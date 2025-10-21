use crate::snapshot::{
    AccountSnapshot,
    Snapshot,
};

use fuels::{
    prelude::*,
    types::Identity,
};
use generated_abi::strapped_types::Strap;

pub trait SnapshotStorage {
    fn latest_snapshot(&self) -> crate::Result<(Snapshot, u32)>;
    fn latest_account_snapshot(
        &self,
        account: &Identity,
    ) -> crate::Result<(AccountSnapshot, u32)>;
    fn get_snapshot_at(&self, height: u32) -> crate::Result<Snapshot>;
    fn get_account_snapshot_at(
        &self,
        account: &Identity,
        height: u32,
    ) -> crate::Result<AccountSnapshot>;
    fn update_snapshot(&mut self, snapshot: &Snapshot, height: u32) -> crate::Result<()>;
    fn update_account_snapshot(
        &mut self,
        account_snapshot: &AccountSnapshot,
        height: u32,
    ) -> crate::Result<()>;
    fn roll_back_snapshots(&mut self, to_height: u32) -> crate::Result<()>;
}

pub trait MetadataStorage {
    fn strap_asset_id(&self, strap_id: &AssetId) -> crate::Result<Option<Strap>>;
    fn record_new_asset_id(
        &mut self,
        strap_id: &AssetId,
        strap: &Strap,
    ) -> crate::Result<()>;
}
