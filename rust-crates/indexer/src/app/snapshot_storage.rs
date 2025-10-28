use crate::snapshot::{
    AccountSnapshot,
    HistoricalSnapshot,
    OverviewSnapshot,
};

use crate::events::Strap;
use fuels::{
    prelude::*,
    types::Identity,
};

pub trait SnapshotStorage {
    /// retrieve latest snapshot along with its block height
    fn latest_snapshot(&self) -> crate::Result<(OverviewSnapshot, u32)>;

    /// retrieve latest account snapshot along with its block height
    fn latest_account_snapshot(
        &self,
        account: &Identity,
    ) -> crate::Result<Option<(AccountSnapshot, u32)>>;

    /// write or overwrite snapshot at given block height
    fn update_snapshot(
        &mut self,
        snapshot: &OverviewSnapshot,
        height: u32,
    ) -> crate::Result<()>;

    /// write or overwrite account snapshot at given block height
    fn update_account_snapshot(
        &mut self,
        account: &Identity,
        game_id: u32,
        account_snapshot: &AccountSnapshot,
        height: u32,
    ) -> crate::Result<()>;

    /// roll back snapshots to given block height (deleting any snapshots above that height)
    fn roll_back_snapshots(&mut self, to_height: u32) -> crate::Result<()>;

    /// retrieve historical snapshot for given game id
    fn historical_snapshots(&self, game_id: u32) -> crate::Result<HistoricalSnapshot>;

    /// write or overwrite historical snapshot for given game id
    fn write_historical_snapshot(
        &mut self,
        game_id: u32,
        snapshot: &HistoricalSnapshot,
    ) -> crate::Result<()>;
}

pub trait MetadataStorage {
    fn strap_asset_id(&self, strap_id: &AssetId) -> crate::Result<Option<Strap>>;
    fn record_new_asset_id(
        &mut self,
        strap_id: &AssetId,
        strap: &Strap,
    ) -> crate::Result<()>;
}
