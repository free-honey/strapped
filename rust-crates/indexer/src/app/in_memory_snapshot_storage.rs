use crate::{
    app::snapshot_storage::SnapshotStorage,
    snapshot::{
        AccountSnapshot,
        HistoricalSnapshot,
        OverviewSnapshot,
    },
};
use fuels::types::Identity;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

#[derive(Clone)]
pub struct InMemorySnapshotStorage {
    snapshot: Arc<Mutex<Option<(OverviewSnapshot, u32)>>>,
    account_snapshots: Arc<Mutex<HashMap<String, (AccountSnapshot, u32)>>>,
    historical_snapshots: Arc<Mutex<HashMap<u32, HistoricalSnapshot>>>,
}

impl InMemorySnapshotStorage {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(None)),
            account_snapshots: Arc::new(Mutex::new(HashMap::new())),
            historical_snapshots: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_with_snapshot(snapshot: OverviewSnapshot, height: u32) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(Some((snapshot, height)))),
            account_snapshots: Arc::new(Mutex::new(HashMap::new())),
            historical_snapshots: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn snapshot(&self) -> Arc<Mutex<Option<(OverviewSnapshot, u32)>>> {
        self.snapshot.clone()
    }

    pub fn account_snapshots(
        &self,
    ) -> Arc<Mutex<HashMap<String, (AccountSnapshot, u32)>>> {
        self.account_snapshots.clone()
    }

    pub fn historical_snapshots(&self) -> Arc<Mutex<HashMap<u32, HistoricalSnapshot>>> {
        self.historical_snapshots.clone()
    }

    pub fn identity_key(account: &Identity) -> String {
        format!("{:?}", account)
    }
}

impl SnapshotStorage for InMemorySnapshotStorage {
    fn latest_snapshot(&self) -> crate::Result<(OverviewSnapshot, u32)> {
        let guard = self.snapshot.lock().unwrap();
        match &*guard {
            Some(snapshot) => Ok(snapshot.clone()),
            None => Err(anyhow::anyhow!("No snapshot found")),
        }
    }

    fn latest_account_snapshot(
        &self,
        account: &Identity,
    ) -> crate::Result<(AccountSnapshot, u32)> {
        let key = Self::identity_key(account);
        let guard = self.account_snapshots.lock().unwrap();
        guard
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No account snapshot found"))
    }

    fn update_snapshot(
        &mut self,
        snapshot: &OverviewSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        let mut guard = self.snapshot.lock().unwrap();
        *guard = Some((snapshot.clone(), height));
        Ok(())
    }

    fn update_account_snapshot(
        &mut self,
        account: &Identity,
        account_snapshot: &AccountSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        let key = Self::identity_key(account);
        let mut guard = self.account_snapshots.lock().unwrap();
        guard.insert(key, (account_snapshot.clone(), height));
        Ok(())
    }

    fn roll_back_snapshots(&mut self, _to_height: u32) -> crate::Result<()> {
        todo!()
    }

    fn historical_snapshots(&self, game_id: u32) -> crate::Result<HistoricalSnapshot> {
        let guard = self.historical_snapshots.lock().unwrap();
        guard
            .get(&game_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No historical snapshot found"))
    }

    fn write_historical_snapshot(
        &mut self,
        game_id: u32,
        snapshot: &HistoricalSnapshot,
    ) -> crate::Result<()> {
        let mut guard = self.historical_snapshots.lock().unwrap();
        guard.insert(game_id, snapshot.clone());
        Ok(())
    }
}
