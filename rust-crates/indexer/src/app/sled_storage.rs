// Sled-backed storage implementations for snapshot and metadata persistence.
use crate::{
    app::snapshot_storage::{
        MetadataStorage,
        SnapshotStorage,
    },
    events::Strap,
    snapshot::{
        AccountSnapshot,
        HistoricalSnapshot,
        OverviewSnapshot,
    },
};
use anyhow::{
    Context,
    anyhow,
};
use fuel_core::types::fuel_tx::AssetId;
use fuels::types::Identity;
use serde::{
    Deserialize,
    Serialize,
    de::DeserializeOwned,
};
use sled::{
    Config,
    Db,
    Tree,
};
use std::{
    convert::TryInto,
    path::Path,
    str::FromStr,
};

const LATEST_HEIGHT_KEY: &[u8] = b"latest_height";

#[derive(Clone)]
pub struct SledSnapshotStorage {
    overview_tree: Tree,
    overview_meta: Tree,
    account_tree: Tree,
    historical_tree: Tree,
}

#[derive(Clone)]
pub struct SledMetadataStorage {
    tree: Tree,
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotRecord<T> {
    snapshot: T,
    height: u32,
}

impl SledSnapshotStorage {
    pub fn new(db: &Db) -> crate::Result<Self> {
        let overview_tree = db
            .open_tree("snapshot_overview")
            .context("open snapshot_overview tree")?;
        let overview_meta = db
            .open_tree("snapshot_overview_meta")
            .context("open snapshot_overview_meta tree")?;
        let account_tree = db
            .open_tree("account_snapshots")
            .context("open account_snapshots tree")?;
        let historical_tree = db
            .open_tree("historical_snapshots")
            .context("open historical_snapshots tree")?;

        Ok(Self {
            overview_tree,
            overview_meta,
            account_tree,
            historical_tree,
        })
    }

    pub fn open<P: AsRef<Path>>(path: P) -> crate::Result<(Self, SledMetadataStorage)> {
        let config = Config::default().path(path);
        let db = config.open().context("open sled database")?;
        let snapshots = Self::new(&db)?;
        let metadata = SledMetadataStorage::new(&db)?;
        Ok((snapshots, metadata))
    }

    /// Remove all snapshots (overview and account) with a block height greater than
    /// or equal to `from_height`.
    pub fn prune_from(&mut self, from_height: u32) -> crate::Result<()> {
        if from_height == 0 {
            self.overview_tree
                .clear()
                .context("clear overview snapshots during prune_from(0)")?;
            self.overview_tree
                .flush()
                .context("flush overview snapshots during prune_from(0)")?;

            self.account_tree
                .clear()
                .context("clear account snapshots during prune_from(0)")?;
            self.account_tree
                .flush()
                .context("flush account snapshots during prune_from(0)")?;

            self.clear_latest_height()?;

            // Historical snapshots are game-scoped and immutable from the perspective of
            // rollbacks, so we leave them untouched even when starting from genesis.
            return Ok(());
        }

        let rollback_to = from_height
            .checked_sub(1)
            .expect("from_height > 0 so subtraction cannot underflow");
        self.roll_back_snapshots(rollback_to)
    }

    fn latest_height(&self) -> crate::Result<Option<u32>> {
        match self.overview_meta.get(LATEST_HEIGHT_KEY)? {
            Some(bytes) => {
                let arr: [u8; 4] = bytes
                    .as_ref()
                    .try_into()
                    .context("latest height should be 4 bytes")?;
                Ok(Some(u32::from_be_bytes(arr)))
            }
            None => Ok(None),
        }
    }

    fn set_latest_height(&self, height: u32) -> crate::Result<()> {
        let height_bytes = height.to_be_bytes();
        self.overview_meta
            .insert(LATEST_HEIGHT_KEY, height_bytes.as_slice())
            .context("write latest overview height")?;
        self.overview_meta
            .flush()
            .context("flush latest overview height")?;
        Ok(())
    }

    fn clear_latest_height(&self) -> crate::Result<()> {
        self.overview_meta
            .remove(LATEST_HEIGHT_KEY)
            .context("remove latest overview height")?;
        self.overview_meta
            .flush()
            .context("flush latest overview height")?;
        Ok(())
    }

    fn load_latest_overview(
        &self,
    ) -> crate::Result<Option<SnapshotRecord<OverviewSnapshot>>> {
        let Some(height) = self.latest_height()? else {
            return Ok(None);
        };
        self.overview_at_height(height)
    }

    fn overview_at_height(
        &self,
        height: u32,
    ) -> crate::Result<Option<SnapshotRecord<OverviewSnapshot>>> {
        let key = height.to_be_bytes();
        let value = match self.overview_tree.get(key)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let record = deserialize::<SnapshotRecord<OverviewSnapshot>>(value.as_ref())?;
        Ok(Some(record))
    }

    fn account_key(account: &Identity, game_id: u32) -> Vec<u8> {
        format!("{}|{}", Self::identity_key(account), game_id).into_bytes()
    }

    fn identity_key(account: &Identity) -> String {
        format!("{:?}", account)
    }

    fn serialize_record<T: Serialize>(value: &T, label: &str) -> crate::Result<Vec<u8>> {
        serde_json::to_vec(value).with_context(|| format!("serialize {label}"))
    }

    fn persist_overview(
        &self,
        record: &SnapshotRecord<OverviewSnapshot>,
    ) -> crate::Result<()> {
        let key = record.height.to_be_bytes();
        let bytes = Self::serialize_record(record, "overview snapshot record")?;
        self.overview_tree
            .insert(key, bytes)
            .context("persist overview snapshot")?;
        self.overview_tree
            .flush()
            .context("flush overview snapshot")?;
        Ok(())
    }

    fn persist_account(
        &self,
        key: Vec<u8>,
        record: &SnapshotRecord<AccountSnapshot>,
    ) -> crate::Result<()> {
        let bytes = Self::serialize_record(record, "account snapshot record")?;
        self.account_tree
            .insert(key, bytes)
            .context("persist account snapshot")?;
        self.account_tree
            .flush()
            .context("flush account snapshots")?;
        Ok(())
    }

    fn remove_account_entry(&self, key: &[u8]) -> crate::Result<()> {
        self.account_tree
            .remove(key)
            .context("remove account snapshot entry")?;
        Ok(())
    }
}

impl SnapshotStorage for SledSnapshotStorage {
    fn latest_snapshot(&self) -> crate::Result<(OverviewSnapshot, u32)> {
        match self.load_latest_overview()? {
            Some(record) => Ok((record.snapshot, record.height)),
            None => Err(anyhow!("No snapshot found")),
        }
    }

    fn latest_account_snapshot(
        &self,
        account: &Identity,
    ) -> crate::Result<Option<(AccountSnapshot, u32)>> {
        let Some(record) = self.load_latest_overview()? else {
            return Ok(None);
        };
        self.account_snapshot_at(account, record.snapshot.game_id)
    }

    fn account_snapshot_at(
        &self,
        account: &Identity,
        game_id: u32,
    ) -> crate::Result<Option<(AccountSnapshot, u32)>> {
        let key = Self::account_key(account, game_id);
        let value = match self.account_tree.get(key)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let record = deserialize::<SnapshotRecord<AccountSnapshot>>(value.as_ref())?;
        Ok(Some((record.snapshot, record.height)))
    }

    fn update_snapshot(
        &mut self,
        snapshot: &OverviewSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        let record = SnapshotRecord {
            snapshot: snapshot.clone(),
            height,
        };
        self.persist_overview(&record)?;
        self.set_latest_height(height)?;
        Ok(())
    }

    fn update_account_snapshot(
        &mut self,
        account: &Identity,
        game_id: u32,
        account_snapshot: &AccountSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        let record = SnapshotRecord {
            snapshot: account_snapshot.clone(),
            height,
        };
        let key = Self::account_key(account, game_id);
        self.persist_account(key, &record)
    }

    fn roll_back_snapshots(&mut self, to_height: u32) -> crate::Result<()> {
        let mut latest_candidate = None;

        for entry in self.overview_tree.iter() {
            let (key, _) = entry.context("iterate overview snapshots")?;
            let height = u32::from_be_bytes(
                key.as_ref()
                    .try_into()
                    .context("overview snapshot key must be 4 bytes")?,
            );
            if height > to_height {
                self.overview_tree
                    .remove(&key)
                    .context("remove overview snapshot during rollback")?;
            } else {
                latest_candidate = Some(height);
            }
        }
        self.overview_tree
            .flush()
            .context("flush overview snapshots")?;

        if let Some(height) = latest_candidate {
            self.set_latest_height(height)?;
        } else {
            self.clear_latest_height()?;
        }

        for entry in self.account_tree.iter() {
            let (key, value) = entry.context("iterate account snapshots")?;
            let record = deserialize::<SnapshotRecord<AccountSnapshot>>(value.as_ref())?;
            if record.height > to_height {
                self.remove_account_entry(key.as_ref())?;
            }
        }
        self.account_tree
            .flush()
            .context("flush account snapshots")?;

        // Historical snapshots are keyed by game id and are immutable once written,
        // so we leave them untouched during rollback.
        Ok(())
    }

    fn historical_snapshots(&self, game_id: u32) -> crate::Result<HistoricalSnapshot> {
        let key = game_id.to_be_bytes();
        let value = self
            .historical_tree
            .get(key)?
            .ok_or_else(|| anyhow!("No historical snapshot found for game {game_id}"))?;
        let snapshot = deserialize::<HistoricalSnapshot>(value.as_ref())?;
        Ok(snapshot)
    }

    fn write_historical_snapshot(
        &mut self,
        game_id: u32,
        snapshot: &HistoricalSnapshot,
    ) -> crate::Result<()> {
        let key = game_id.to_be_bytes();
        let bytes = Self::serialize_record(snapshot, "historical snapshot record")?;
        self.historical_tree
            .insert(key, bytes)
            .context("persist historical snapshot")?;
        self.historical_tree
            .flush()
            .context("flush historical snapshots")?;
        Ok(())
    }
}

impl SledMetadataStorage {
    pub fn new(db: &Db) -> crate::Result<Self> {
        let tree = db.open_tree("metadata").context("open metadata tree")?;
        Ok(Self { tree })
    }

    fn strap_key(strap_id: &AssetId) -> crate::Result<Vec<u8>> {
        // Use debug formatting for a stable textual key without new dependencies.
        Ok(format!("{:?}", strap_id).into_bytes())
    }
}

impl MetadataStorage for SledMetadataStorage {
    fn strap_asset_id(&self, strap_id: &AssetId) -> crate::Result<Option<Strap>> {
        let key = Self::strap_key(strap_id)?;
        let value = match self.tree.get(key)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let strap = deserialize::<Strap>(value.as_ref())?;
        Ok(Some(strap))
    }

    fn record_new_asset_id(
        &mut self,
        strap_id: &AssetId,
        strap: &Strap,
    ) -> crate::Result<()> {
        let key = Self::strap_key(strap_id)?;
        let bytes = SledSnapshotStorage::serialize_record(strap, "strap metadata")?;
        self.tree
            .insert(key, bytes)
            .context("persist strap metadata")?;
        self.tree.flush().context("flush metadata tree")?;
        Ok(())
    }

    fn all_known_strap_asset_ids(&self) -> crate::Result<Vec<AssetId>> {
        let mut asset_ids = Vec::new();
        for entry in self.tree.iter() {
            let (key, _) = entry.context("iterate strap metadata entries")?;
            let key_str = std::str::from_utf8(key.as_ref())
                .context("metadata key is not valid UTF-8")?;
            let asset_id = AssetId::from_str(key_str)
                .map_err(|_| anyhow!("invalid asset id metadata key: {key_str}"))?;
            asset_ids.push(asset_id);
        }
        Ok(asset_ids)
    }

    fn all_known_straps(&self) -> crate::Result<Vec<(AssetId, Strap)>> {
        let mut straps = Vec::new();
        for entry in self.tree.iter() {
            let (key, value) = entry.context("iterate strap metadata entries")?;
            let key_str = std::str::from_utf8(key.as_ref())
                .context("metadata key is not valid UTF-8")?;
            let asset_id = AssetId::from_str(key_str)
                .map_err(|_| anyhow!("invalid asset id metadata key: {key_str}"))?;
            let strap = deserialize::<Strap>(value.as_ref())
                .context("deserialize strap metadata")?;
            straps.push((asset_id, strap));
        }
        Ok(straps)
    }
}

fn deserialize<T: DeserializeOwned>(bytes: &[u8]) -> crate::Result<T> {
    serde_json::from_slice(bytes).context("deserialize sled record")
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::{
        SledMetadataStorage,
        SledSnapshotStorage,
    };
    use crate::{
        app::snapshot_storage::{
            MetadataStorage,
            SnapshotStorage,
        },
        events::{
            Modifier,
            Strap,
            StrapKind,
        },
        snapshot::{
            AccountSnapshot,
            OverviewSnapshot,
        },
    };
    use fuel_core::types::fuel_tx::AssetId;
    use fuels::types::{
        Address,
        Identity,
    };
    use tempdir::TempDir;

    fn sled_db(temp_dir: &TempDir) -> sled::Db {
        sled::Config::default()
            .path(temp_dir.path())
            .open()
            .expect("open sled db")
    }

    #[test]
    fn sut__when_updating_snapshots_then_latest_state_is_persisted() {
        // given
        let temp_dir = TempDir::new("sled_snapshot_storage").unwrap();
        let db = sled_db(&temp_dir);

        let mut storage = SledSnapshotStorage::new(&db).unwrap();
        let mut snapshot = OverviewSnapshot::default();
        snapshot.game_id = 42;
        let account = Identity::Address(Address::from([0u8; 32]));
        let mut account_snapshot = AccountSnapshot::default();
        account_snapshot.total_chip_bet = 10;

        // when
        storage.update_snapshot(&snapshot, 100).unwrap();
        storage
            .update_account_snapshot(&account, 42, &account_snapshot, 100)
            .unwrap();

        // then
        let (latest, height) = storage.latest_snapshot().unwrap();
        assert_eq!(height, 100);
        assert_eq!(latest.game_id, 42);

        let (loaded_account, account_height) = storage
            .latest_account_snapshot(&account)
            .unwrap()
            .expect("account snapshot exists");
        assert_eq!(account_height, 100);
        assert_eq!(loaded_account.total_chip_bet, 10);
    }

    #[test]
    fn sut__when_rolling_back_then_newer_entries_are_pruned() {
        // given
        let temp_dir = TempDir::new("sled_snapshot_storage_rollback").unwrap();
        let db = sled_db(&temp_dir);

        let mut storage = SledSnapshotStorage::new(&db).unwrap();
        let mut snapshot_one = OverviewSnapshot::default();
        snapshot_one.game_id = 1;
        let account = Identity::Address(Address::from([1u8; 32]));
        let mut account_snapshot = AccountSnapshot::default();
        account_snapshot.total_chip_won = 5;
        let mut snapshot_two = OverviewSnapshot::default();
        snapshot_two.game_id = 2;
        let mut account_snapshot_two = AccountSnapshot::default();
        account_snapshot_two.total_chip_won = 15;

        storage.update_snapshot(&snapshot_one, 10).unwrap();
        storage
            .update_account_snapshot(&account, 1, &account_snapshot, 10)
            .unwrap();
        storage.update_snapshot(&snapshot_two, 20).unwrap();
        storage
            .update_account_snapshot(&account, 2, &account_snapshot_two, 20)
            .unwrap();

        // when
        storage.roll_back_snapshots(15).unwrap();

        // then
        let (latest, height) = storage.latest_snapshot().unwrap();
        assert_eq!(height, 10);
        assert_eq!(latest.game_id, 1);

        let account_latest = storage.latest_account_snapshot(&account).unwrap();
        assert!(account_latest.is_some());
        let (account_snapshot, account_height) = account_latest.unwrap();
        assert_eq!(account_height, 10);
        assert_eq!(account_snapshot.total_chip_won, 5);
    }

    #[test]
    fn sut__when_pruning_from_height_then_entries_at_or_above_are_removed() {
        // given
        let temp_dir = TempDir::new("sled_snapshot_storage_prune").unwrap();
        let db = sled_db(&temp_dir);

        let mut storage = SledSnapshotStorage::new(&db).unwrap();
        let mut snapshot_one = OverviewSnapshot::default();
        snapshot_one.game_id = 1;
        let mut snapshot_two = OverviewSnapshot::default();
        snapshot_two.game_id = 2;

        storage.update_snapshot(&snapshot_one, 10).unwrap();
        storage.update_snapshot(&snapshot_two, 20).unwrap();

        let account = Identity::Address(Address::from([2u8; 32]));
        let mut account_snapshot = AccountSnapshot::default();
        account_snapshot.total_chip_bet = 3;
        storage
            .update_account_snapshot(&account, 1, &account_snapshot, 10)
            .unwrap();

        let mut account_snapshot_two = AccountSnapshot::default();
        account_snapshot_two.total_chip_bet = 6;
        storage
            .update_account_snapshot(&account, 2, &account_snapshot_two, 20)
            .unwrap();

        // when
        storage.prune_from(20).unwrap();

        // then
        let (latest, height) = storage.latest_snapshot().unwrap();
        assert_eq!(height, 10);
        assert_eq!(latest.game_id, 1);

        let latest_account = storage.latest_account_snapshot(&account).unwrap();
        assert!(latest_account.is_some());
        let (account_snapshot, account_height) = latest_account.unwrap();
        assert_eq!(account_height, 10);
        assert_eq!(account_snapshot.total_chip_bet, 3);

        // when
        storage.prune_from(0).unwrap();

        // then
        assert!(storage.latest_snapshot().is_err());
        let latest_account = storage.latest_account_snapshot(&account).unwrap();
        assert!(latest_account.is_none());
    }

    #[test]
    fn sut__when_recording_metadata_then_lookup_returns_value() {
        // given
        let temp_dir = TempDir::new("sled_metadata_storage").unwrap();
        let db = sled_db(&temp_dir);
        let mut metadata = SledMetadataStorage::new(&db).unwrap();

        let strap = Strap::new(1, StrapKind::Hat, Modifier::Lucky);
        let asset_id = AssetId::from([9u8; 32]);

        assert!(metadata.strap_asset_id(&asset_id).unwrap().is_none());

        // when
        metadata.record_new_asset_id(&asset_id, &strap).unwrap();

        // then
        let loaded = metadata
            .strap_asset_id(&asset_id)
            .unwrap()
            .expect("metadata stored");
        assert_eq!(loaded, strap);
    }

    #[test]
    fn all_known_strap_asset_ids__returns_all_recorded_ids() {
        // given
        let temp_dir = TempDir::new("sled_metadata_known_ids").unwrap();
        let db = sled_db(&temp_dir);
        let mut metadata = SledMetadataStorage::new(&db).unwrap();
        let strap_a = Strap::new(1, StrapKind::Hat, Modifier::Lucky);
        let strap_b = Strap::new(3, StrapKind::Scarf, Modifier::Holy);
        let asset_id_a = AssetId::from([3u8; 32]);
        let asset_id_b = AssetId::from([4u8; 32]);
        metadata.record_new_asset_id(&asset_id_a, &strap_a).unwrap();
        metadata.record_new_asset_id(&asset_id_b, &strap_b).unwrap();

        // when
        let mut known = metadata.all_known_strap_asset_ids().unwrap();
        known.sort();

        // then
        assert_eq!(known, vec![asset_id_a, asset_id_b]);
    }

    #[test]
    fn all_known_straps__returns_pairs_with_metadata() {
        // given
        let temp_dir = TempDir::new("sled_metadata_with_straps").unwrap();
        let db = sled_db(&temp_dir);
        let mut metadata = SledMetadataStorage::new(&db).unwrap();
        let strap_a = Strap::new(1, StrapKind::Hat, Modifier::Lucky);
        let strap_b = Strap::new(3, StrapKind::Scarf, Modifier::Holy);
        let asset_id_a = AssetId::from([7u8; 32]);
        let asset_id_b = AssetId::from([8u8; 32]);
        metadata.record_new_asset_id(&asset_id_a, &strap_a).unwrap();
        metadata.record_new_asset_id(&asset_id_b, &strap_b).unwrap();

        // when
        let mut known = metadata.all_known_straps().unwrap();
        known.sort_by_key(|(asset_id, _)| *asset_id);

        // then
        assert_eq!(known, vec![(asset_id_a, strap_a), (asset_id_b, strap_b)]);
    }
}
