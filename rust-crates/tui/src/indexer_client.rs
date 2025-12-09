use std::fmt;

use color_eyre::eyre::{
    Result,
    WrapErr,
    eyre,
};
use fuels::types::{
    AssetId,
    Identity,
};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json;
use strapped_contract::strapped_types as strapped;

#[derive(Clone)]
pub struct IndexerClient {
    base_url: String,
    http: reqwest::Client,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OverviewData {
    pub game_id: u32,
    pub rolls: Vec<strapped::Roll>,
    pub pot_size: u64,
    pub chips_owed: u64,
    pub total_chip_bets: u64,
    pub current_block_height: u32,
    pub next_roll_height: Option<u32>,
    pub rewards: Vec<(strapped::Roll, strapped::Strap, u64)>,
    pub modifier_shop: Vec<(strapped::Roll, strapped::Roll, strapped::Modifier, bool)>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AccountData {
    pub per_roll_bets: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>,
    pub strap_totals: Vec<(strapped::Strap, u64)>,
    pub total_chip_bet: u64,
    pub total_chip_won: u64,
    pub claimed_rewards: Option<(u64, Vec<(strapped::Strap, u64)>)>,
    pub block_height: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct HistoricalData {
    pub game_id: u32,
    pub rolls: Vec<strapped::Roll>,
    pub modifiers: Vec<(strapped::Roll, strapped::Modifier, u32)>,
    pub strap_rewards: Vec<(strapped::Roll, strapped::Strap, u64)>,
}

impl IndexerClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let http = reqwest::Client::builder()
            .build()
            .wrap_err("failed to build HTTP client for indexer")?;
        Ok(Self { base_url, http })
    }

    pub async fn latest_overview(&self) -> Result<Option<OverviewData>> {
        let url = format!("{}/snapshot/latest", self.base_url);
        let res = self
            .http
            .get(url)
            .send()
            .await
            .wrap_err("indexer request failed")?;
        let status = res.status();
        let bytes = res
            .bytes()
            .await
            .wrap_err("failed to read indexer response body")?;
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            return Err(eyre!(
                "indexer responded with {status} when fetching latest snapshot: {body}"
            ));
        }
        let dto: LatestSnapshotDto = serde_json::from_slice(&bytes)
            .wrap_err("invalid indexer overview payload")?;
        Ok(Some(dto.into()))
    }

    pub async fn latest_account_snapshot(
        &self,
        identity: &Identity,
    ) -> Result<Option<AccountData>> {
        let identity_path = Self::identity_path(identity)?;
        let url = format!("{}/account/{}", self.base_url, identity_path);
        self.fetch_account_data(url).await
    }

    pub async fn historical_snapshot(
        &self,
        game_id: u32,
    ) -> Result<Option<HistoricalData>> {
        let url = format!("{}/historical/{}", self.base_url, game_id);
        let res = self
            .http
            .get(url)
            .send()
            .await
            .wrap_err("indexer request failed")?;
        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let dto: Option<HistoricalSnapshotDto> = res
            .json()
            .await
            .wrap_err("invalid indexer historical payload")?;
        Ok(dto.map(Into::into))
    }

    pub async fn historical_account_snapshot(
        &self,
        identity: &Identity,
        game_id: u32,
    ) -> Result<Option<AccountData>> {
        let identity_path = Self::identity_path(identity)?;
        let url = format!("{}/account/{}/{}", self.base_url, identity_path, game_id);
        self.fetch_account_data(url).await
    }

    pub async fn all_known_straps(&self) -> Result<Vec<(AssetId, strapped::Strap)>> {
        let url = format!("{}/straps", self.base_url);
        let res = self
            .http
            .get(url)
            .send()
            .await
            .wrap_err("indexer request failed")?;
        let status = res.status();
        if !status.is_success() {
            let body = res
                .text()
                .await
                .unwrap_or_else(|_| "<unavailable body>".to_string());
            return Err(eyre!(
                "indexer responded with {status} when fetching strap metadata: {body}"
            ));
        }
        let dtos: Vec<StrapMetadataDto> = res
            .json()
            .await
            .wrap_err("invalid indexer strap metadata payload")?;
        Ok(dtos
            .into_iter()
            .map(|dto| (dto.asset_id, dto.strap.into()))
            .collect())
    }

    async fn fetch_account_data(&self, url: String) -> Result<Option<AccountData>> {
        let res = self
            .http
            .get(url)
            .send()
            .await
            .wrap_err("indexer request failed")?;
        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let dto: Option<LatestAccountSnapshotDto> = res
            .json()
            .await
            .wrap_err("invalid indexer account payload")?;
        Ok(dto.map(Into::into))
    }

    fn identity_path(identity: &Identity) -> Result<String> {
        match identity {
            Identity::Address(address) => Ok(address.to_string()),
            other => Err(eyre!("unsupported identity for indexer: {other:?}")),
        }
    }
}

#[derive(Deserialize)]
struct LatestSnapshotDto {
    snapshot: OverviewSnapshotDto,
    block_height: u32,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct OverviewSnapshotDto {
    game_id: u32,
    rolls: Vec<RollDto>,
    pot_size: u64,
    chips_owed: u64,
    current_block_height: u32,
    next_roll_height: Option<u32>,
    rewards: Vec<(RollDto, StrapDto, u64)>,
    total_chip_bets: u64,
    specific_bets: Vec<(u64, Vec<(StrapDto, u64)>)>,
    modifiers_active: Vec<Option<ModifierDto>>,
    modifier_shop: Vec<(RollDto, RollDto, ModifierDto, bool)>,
}

#[derive(Deserialize)]
struct LatestAccountSnapshotDto {
    snapshot: AccountSnapshotDto,
    block_height: u32,
}

#[derive(Deserialize)]
struct StrapMetadataDto {
    asset_id: AssetId,
    strap: StrapDto,
}

#[derive(Deserialize)]
struct AccountSnapshotDto {
    total_chip_bet: u64,
    strap_bets: Vec<(StrapDto, u64)>,
    total_chip_won: u64,
    claimed_rewards: Option<(u64, Vec<(StrapDto, u64)>)>,
    per_roll_bets: Vec<AccountRollBetsDto>,
}

#[derive(Deserialize)]
struct AccountRollBetsDto {
    roll: RollDto,
    bets: Vec<AccountBetPlacementDto>,
}

#[derive(Deserialize)]
struct HistoricalSnapshotDto {
    snapshot: HistoricalSnapshotInnerDto,
}

#[derive(Deserialize)]
struct HistoricalSnapshotInnerDto {
    game_id: u32,
    rolls: Vec<RollDto>,
    modifiers: Vec<ActiveModifierDto>,
    strap_rewards: Vec<(RollDto, StrapDto, u64)>,
}

#[derive(Deserialize)]
struct ActiveModifierDto {
    roll_index: u32,
    modifier: ModifierDto,
    modifier_roll: RollDto,
}

#[derive(Deserialize)]
struct AccountBetPlacementDto {
    bet_roll_index: u32,
    amount: u64,
    kind: AccountBetKindDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
enum AccountBetKindDto {
    Chip,
    Strap(StrapDto),
}

#[derive(Deserialize, Clone)]
struct StrapDto {
    level: u8,
    kind: StrapKindDto,
    modifier: ModifierDto,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
enum StrapKindDto {
    Shirt,
    Pants,
    Shoes,
    Dress,
    Hat,
    Glasses,
    Watch,
    Ring,
    Necklace,
    Earring,
    Bracelet,
    Tattoo,
    Skirt,
    Piercing,
    Coat,
    Scarf,
    Gloves,
    Gown,
    Belt,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
enum ModifierDto {
    Nothing,
    Burnt,
    Lucky,
    Holy,
    Holey,
    Scotch,
    Soaked,
    Moldy,
    Starched,
    Evil,
    Groovy,
    Delicate,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
enum RollDto {
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Eleven,
    Twelve,
}

impl From<LatestSnapshotDto> for OverviewData {
    fn from(dto: LatestSnapshotDto) -> Self {
        let mut overview = OverviewData {
            game_id: dto.snapshot.game_id,
            rolls: dto.snapshot.rolls.into_iter().map(Into::into).collect(),
            pot_size: dto.snapshot.pot_size,
            chips_owed: dto.snapshot.chips_owed,
            total_chip_bets: dto.snapshot.total_chip_bets,
            current_block_height: dto.snapshot.current_block_height,
            next_roll_height: dto.snapshot.next_roll_height,
            rewards: dto
                .snapshot
                .rewards
                .into_iter()
                .map(|(roll, strap, amount)| (roll.into(), strap.into(), amount))
                .collect(),
            modifier_shop: dto
                .snapshot
                .modifier_shop
                .into_iter()
                .map(|(trigger, target, modifier, active)| {
                    (trigger.into(), target.into(), modifier.into(), active)
                })
                .collect(),
        };
        overview.current_block_height =
            dto.block_height.max(overview.current_block_height);
        overview
    }
}

impl From<LatestAccountSnapshotDto> for AccountData {
    fn from(dto: LatestAccountSnapshotDto) -> Self {
        let per_roll_bets = dto
            .snapshot
            .per_roll_bets
            .into_iter()
            .map(|entry| {
                let roll: strapped::Roll = entry.roll.into();
                let bets = entry
                    .bets
                    .into_iter()
                    .map(|bet| {
                        let bet_kind: strapped::Bet = bet.kind.into();
                        (bet_kind, bet.amount, bet.bet_roll_index)
                    })
                    .collect();
                (roll, bets)
            })
            .collect();
        AccountData {
            per_roll_bets,
            strap_totals: dto
                .snapshot
                .strap_bets
                .into_iter()
                .map(|(strap, amount)| (strap.into(), amount))
                .collect(),
            total_chip_bet: dto.snapshot.total_chip_bet,
            total_chip_won: dto.snapshot.total_chip_won,
            claimed_rewards: dto.snapshot.claimed_rewards.map(|(chips, straps)| {
                (
                    chips,
                    straps.into_iter().map(|(s, n)| (s.into(), n)).collect(),
                )
            }),
            block_height: dto.block_height,
        }
    }
}

impl From<HistoricalSnapshotDto> for HistoricalData {
    fn from(dto: HistoricalSnapshotDto) -> Self {
        HistoricalData {
            game_id: dto.snapshot.game_id,
            rolls: dto.snapshot.rolls.into_iter().map(Into::into).collect(),
            modifiers: dto
                .snapshot
                .modifiers
                .into_iter()
                .map(|entry| {
                    (
                        entry.modifier_roll.into(),
                        entry.modifier.into(),
                        entry.roll_index,
                    )
                })
                .collect(),
            strap_rewards: dto
                .snapshot
                .strap_rewards
                .into_iter()
                .map(|(roll, strap, cost)| (roll.into(), strap.into(), cost))
                .collect(),
        }
    }
}

impl From<RollDto> for strapped::Roll {
    fn from(value: RollDto) -> Self {
        match value {
            RollDto::Two => strapped::Roll::Two,
            RollDto::Three => strapped::Roll::Three,
            RollDto::Four => strapped::Roll::Four,
            RollDto::Five => strapped::Roll::Five,
            RollDto::Six => strapped::Roll::Six,
            RollDto::Seven => strapped::Roll::Seven,
            RollDto::Eight => strapped::Roll::Eight,
            RollDto::Nine => strapped::Roll::Nine,
            RollDto::Ten => strapped::Roll::Ten,
            RollDto::Eleven => strapped::Roll::Eleven,
            RollDto::Twelve => strapped::Roll::Twelve,
        }
    }
}

impl From<ModifierDto> for strapped::Modifier {
    fn from(value: ModifierDto) -> Self {
        match value {
            ModifierDto::Nothing => strapped::Modifier::Nothing,
            ModifierDto::Burnt => strapped::Modifier::Burnt,
            ModifierDto::Lucky => strapped::Modifier::Lucky,
            ModifierDto::Holy => strapped::Modifier::Holy,
            ModifierDto::Holey => strapped::Modifier::Holey,
            ModifierDto::Scotch => strapped::Modifier::Scotch,
            ModifierDto::Soaked => strapped::Modifier::Soaked,
            ModifierDto::Moldy => strapped::Modifier::Moldy,
            ModifierDto::Starched => strapped::Modifier::Starched,
            ModifierDto::Evil => strapped::Modifier::Evil,
            ModifierDto::Groovy => strapped::Modifier::Groovy,
            ModifierDto::Delicate => strapped::Modifier::Delicate,
        }
    }
}

impl From<StrapDto> for strapped::Strap {
    fn from(value: StrapDto) -> Self {
        strapped::Strap {
            level: value.level,
            kind: value.kind.into(),
            modifier: value.modifier.into(),
        }
    }
}

impl From<StrapKindDto> for strapped::StrapKind {
    fn from(value: StrapKindDto) -> Self {
        match value {
            StrapKindDto::Shirt => strapped::StrapKind::Shirt,
            StrapKindDto::Pants => strapped::StrapKind::Pants,
            StrapKindDto::Shoes => strapped::StrapKind::Shoes,
            StrapKindDto::Dress => strapped::StrapKind::Dress,
            StrapKindDto::Hat => strapped::StrapKind::Hat,
            StrapKindDto::Glasses => strapped::StrapKind::Glasses,
            StrapKindDto::Watch => strapped::StrapKind::Watch,
            StrapKindDto::Ring => strapped::StrapKind::Ring,
            StrapKindDto::Necklace => strapped::StrapKind::Necklace,
            StrapKindDto::Earring => strapped::StrapKind::Earring,
            StrapKindDto::Bracelet => strapped::StrapKind::Bracelet,
            StrapKindDto::Tattoo => strapped::StrapKind::Tattoo,
            StrapKindDto::Skirt => strapped::StrapKind::Skirt,
            StrapKindDto::Piercing => strapped::StrapKind::Piercing,
            StrapKindDto::Coat => strapped::StrapKind::Coat,
            StrapKindDto::Scarf => strapped::StrapKind::Scarf,
            StrapKindDto::Gloves => strapped::StrapKind::Gloves,
            StrapKindDto::Gown => strapped::StrapKind::Gown,
            StrapKindDto::Belt => strapped::StrapKind::Belt,
        }
    }
}

impl From<AccountBetKindDto> for strapped::Bet {
    fn from(value: AccountBetKindDto) -> Self {
        match value {
            AccountBetKindDto::Chip => strapped::Bet::Chip,
            AccountBetKindDto::Strap(strap) => strapped::Bet::Strap(strap.into()),
        }
    }
}

impl AccountData {
    pub(crate) fn empty() -> Self {
        AccountData {
            per_roll_bets: all_rolls()
                .into_iter()
                .map(|roll| (roll, Vec::new()))
                .collect(),
            strap_totals: Vec::new(),
            total_chip_bet: 0,
            total_chip_won: 0,
            claimed_rewards: None,
            block_height: 0,
        }
    }
}

fn all_rolls() -> Vec<strapped::Roll> {
    use strapped::Roll::*;
    vec![
        Two, Three, Four, Five, Six, Seven, Eight, Nine, Ten, Eleven, Twelve,
    ]
}

impl fmt::Display for IndexerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.base_url)
    }
}
