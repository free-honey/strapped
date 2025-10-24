use crate::{
    app::snapshot_storage::MetadataStorage,
    events::Strap,
};
use fuel_core::types::fuel_tx::AssetId;
use std::collections::HashMap;

#[derive(Default)]
pub struct InMemoryMetadataStorage {
    straps: HashMap<AssetId, Strap>,
}

impl MetadataStorage for InMemoryMetadataStorage {
    fn strap_asset_id(&self, strap_id: &AssetId) -> crate::Result<Option<Strap>> {
        Ok(self.straps.get(strap_id).cloned())
    }

    fn record_new_asset_id(
        &mut self,
        strap_id: &AssetId,
        strap: &Strap,
    ) -> crate::Result<()> {
        self.straps.insert(*strap_id, strap.clone());
        Ok(())
    }
}
