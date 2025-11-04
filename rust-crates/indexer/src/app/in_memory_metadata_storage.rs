use crate::{
    app::snapshot_storage::MetadataStorage,
    events::Strap,
};
use fuel_core::types::fuel_tx::AssetId;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Clone, Default)]
pub struct InMemoryMetadataStorage {
    straps: Arc<Mutex<HashMap<AssetId, Strap>>>,
}

impl InMemoryMetadataStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn straps(&self) -> Arc<Mutex<HashMap<AssetId, Strap>>> {
        self.straps.clone()
    }
}

impl MetadataStorage for InMemoryMetadataStorage {
    fn strap_asset_id(&self, strap_id: &AssetId) -> crate::Result<Option<Strap>> {
        let guard = self.straps.lock().unwrap();
        Ok(guard.get(strap_id).cloned())
    }

    fn all_known_strap_asset_ids(&self) -> crate::Result<Vec<AssetId>> {
        let guard = self.straps.lock().unwrap();
        Ok(guard.keys().copied().collect())
    }

    fn all_known_straps(&self) -> crate::Result<Vec<(AssetId, Strap)>> {
        let guard = self.straps.lock().unwrap();
        Ok(guard
            .iter()
            .map(|(asset_id, strap)| (*asset_id, strap.clone()))
            .collect())
    }

    fn record_new_asset_id(
        &mut self,
        strap_id: &AssetId,
        strap: &Strap,
    ) -> crate::Result<()> {
        let mut guard = self.straps.lock().unwrap();
        guard.insert(*strap_id, strap.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use crate::events::{
        Modifier,
        Strap,
        StrapKind,
    };

    #[test]
    fn all_known_strap_asset_ids__returns_all_inserted_ids() {
        // given
        let mut storage = InMemoryMetadataStorage::new();
        let strap_a = Strap::new(1, StrapKind::Hat, Modifier::Lucky);
        let strap_b = Strap::new(2, StrapKind::Coat, Modifier::Burnt);
        let asset_id_a = AssetId::from([1u8; 32]);
        let asset_id_b = AssetId::from([2u8; 32]);
        storage.record_new_asset_id(&asset_id_a, &strap_a).unwrap();
        storage.record_new_asset_id(&asset_id_b, &strap_b).unwrap();

        // when
        let mut known = storage.all_known_strap_asset_ids().unwrap();
        known.sort();

        // then
        assert_eq!(known, vec![asset_id_a, asset_id_b]);
    }

    #[test]
    fn all_known_straps__returns_asset_and_metadata_pairs() {
        // given
        let mut storage = InMemoryMetadataStorage::new();
        let strap_a = Strap::new(1, StrapKind::Hat, Modifier::Lucky);
        let strap_b = Strap::new(2, StrapKind::Coat, Modifier::Burnt);
        let asset_id_a = AssetId::from([1u8; 32]);
        let asset_id_b = AssetId::from([2u8; 32]);
        storage.record_new_asset_id(&asset_id_a, &strap_a).unwrap();
        storage.record_new_asset_id(&asset_id_b, &strap_b).unwrap();

        // when
        let mut known = storage.all_known_straps().unwrap();
        known.sort_by_key(|(asset_id, _)| *asset_id);

        // then
        assert_eq!(known, vec![(asset_id_a, strap_a), (asset_id_b, strap_b)]);
    }
}
