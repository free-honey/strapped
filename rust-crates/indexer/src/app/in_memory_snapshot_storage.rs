use crate::{
    app::snapshot_storage::{
        BetHistoryGameIdPage,
        SnapshotStorage,
        SortOrder,
        UnclaimedGameIdPage,
    },
    snapshot::{
        AccountSnapshot,
        HistoricalSnapshot,
        OverviewSnapshot,
    },
};
use fuels::types::Identity;
use std::{
    collections::{
        BTreeMap,
        HashSet,
        HashMap,
    },
    sync::{
        Arc,
        Mutex,
    },
};

type AccountSnapshotMap = HashMap<String, HashMap<u32, (AccountSnapshot, u32)>>;
type SharedAccountSnapshots = Arc<Mutex<AccountSnapshotMap>>;
type SharedHistoricalSnapshots = Arc<Mutex<HashMap<u32, HistoricalSnapshot>>>;
type SharedOverviewSnapshot = Arc<Mutex<Option<(OverviewSnapshot, u32)>>>;
type SharedUnclaimedSnapshots = Arc<Mutex<HashMap<String, BTreeMap<u32, u32>>>>;
type SharedBetHistorySnapshots = Arc<Mutex<HashMap<String, BTreeMap<u32, u32>>>>;
type SharedGameBettors = Arc<Mutex<HashMap<u32, HashSet<String>>>>;

#[derive(Clone)]
pub struct InMemorySnapshotStorage {
    latest_game_id: u32,
    snapshot: SharedOverviewSnapshot,
    account_snapshots: SharedAccountSnapshots,
    historical_snapshots: SharedHistoricalSnapshots,
    unclaimed_snapshots: SharedUnclaimedSnapshots,
    bet_history_snapshots: SharedBetHistorySnapshots,
    game_bettors: SharedGameBettors,
}

impl InMemorySnapshotStorage {
    pub fn new() -> Self {
        Self {
            latest_game_id: 0,
            snapshot: Arc::new(Mutex::new(None)),
            account_snapshots: Arc::new(Mutex::new(HashMap::new())),
            historical_snapshots: Arc::new(Mutex::new(HashMap::new())),
            unclaimed_snapshots: Arc::new(Mutex::new(HashMap::new())),
            bet_history_snapshots: Arc::new(Mutex::new(HashMap::new())),
            game_bettors: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_with_snapshot(snapshot: OverviewSnapshot, height: u32) -> Self {
        Self {
            latest_game_id: 0,
            snapshot: Arc::new(Mutex::new(Some((snapshot, height)))),
            account_snapshots: Arc::new(Mutex::new(HashMap::new())),
            historical_snapshots: Arc::new(Mutex::new(HashMap::new())),
            unclaimed_snapshots: Arc::new(Mutex::new(HashMap::new())),
            bet_history_snapshots: Arc::new(Mutex::new(HashMap::new())),
            game_bettors: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn snapshot(&self) -> SharedOverviewSnapshot {
        self.snapshot.clone()
    }

    pub fn account_snapshots(&self) -> SharedAccountSnapshots {
        self.account_snapshots.clone()
    }

    pub fn historical_snapshots(&self) -> SharedHistoricalSnapshots {
        self.historical_snapshots.clone()
    }

    pub fn unclaimed_snapshots(&self) -> SharedUnclaimedSnapshots {
        self.unclaimed_snapshots.clone()
    }

    pub fn bet_history_snapshots(&self) -> SharedBetHistorySnapshots {
        self.bet_history_snapshots.clone()
    }

    pub fn game_bettors(&self) -> SharedGameBettors {
        self.game_bettors.clone()
    }

    pub fn identity_key(account: &Identity) -> String {
        format!("{:?}", account)
    }

    pub fn identity_address_key(account: &Identity) -> Option<String> {
        match account {
            Identity::Address(address) => Some(address.to_string()),
            _ => None,
        }
    }
}

impl Default for InMemorySnapshotStorage {
    fn default() -> Self {
        Self::new()
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
    ) -> crate::Result<Option<(AccountSnapshot, u32)>> {
        let key = Self::identity_key(account);
        let guard = self.account_snapshots.lock().unwrap();
        let game_id = self.latest_game_id;
        let maybe_snapshot = guard
            .get(&key)
            .and_then(|inner| inner.get(&game_id))
            .cloned();
        Ok(maybe_snapshot)
    }

    fn account_snapshot_at(
        &self,
        account: &Identity,
        game_id: u32,
    ) -> crate::Result<Option<(AccountSnapshot, u32)>> {
        let key = Self::identity_key(account);
        let guard = self.account_snapshots.lock().unwrap();
        let maybe_snapshot = guard
            .get(&key)
            .and_then(|inner| inner.get(&game_id))
            .cloned();
        Ok(maybe_snapshot)
    }

    fn bet_history_game_ids(
        &self,
        account: &Identity,
        order: SortOrder,
        limit: usize,
        cursor: Option<u32>,
    ) -> crate::Result<BetHistoryGameIdPage> {
        let key = Self::identity_key(account);
        let guard = self.bet_history_snapshots.lock().unwrap();
        let Some(entries) = guard.get(&key) else {
            return Ok(BetHistoryGameIdPage {
                game_ids: Vec::new(),
                next_cursor: None,
            });
        };

        let mut game_ids = Vec::new();
        let mut last_game_id = None;
        let mut has_more = false;

        let iter: Box<dyn Iterator<Item = (&u32, &u32)>> = match order {
            SortOrder::Asc => Box::new(entries.iter()),
            SortOrder::Desc => Box::new(entries.iter().rev()),
        };

        for (game_id, _) in iter {
            if let Some(cursor) = cursor {
                match order {
                    SortOrder::Asc if *game_id <= cursor => continue,
                    SortOrder::Desc if *game_id >= cursor => continue,
                    _ => {}
                }
            }

            if game_ids.len() < limit {
                game_ids.push(*game_id);
                last_game_id = Some(*game_id);
            } else {
                has_more = true;
                break;
            }
        }

        Ok(BetHistoryGameIdPage {
            game_ids,
            next_cursor: if has_more { last_game_id } else { None },
        })
    }

    fn record_bet_history(
        &mut self,
        account: &Identity,
        game_id: u32,
        height: u32,
    ) -> crate::Result<()> {
        let key = Self::identity_key(account);
        let mut guard = self.bet_history_snapshots.lock().unwrap();
        let entry = guard.entry(key.clone()).or_default();
        entry.insert(game_id, height);
        drop(guard);

        if let Some(address_key) = Self::identity_address_key(account) {
            let mut bettors = self.game_bettors.lock().unwrap();
            let game_entry = bettors.entry(game_id).or_default();
            game_entry.insert(address_key);
        }
        Ok(())
    }

    fn bettors_for_game(&self, game_id: u32) -> crate::Result<Vec<String>> {
        let guard = self.game_bettors.lock().unwrap();
        let Some(entries) = guard.get(&game_id) else {
            return Ok(Vec::new());
        };
        Ok(entries.iter().cloned().collect())
    }

    fn unclaimed_game_ids(
        &self,
        account: &Identity,
        order: SortOrder,
        limit: usize,
        cursor: Option<u32>,
    ) -> crate::Result<UnclaimedGameIdPage> {
        let key = Self::identity_key(account);
        let guard = self.unclaimed_snapshots.lock().unwrap();
        let Some(entries) = guard.get(&key) else {
            return Ok(UnclaimedGameIdPage {
                game_ids: Vec::new(),
                next_cursor: None,
            });
        };

        let mut game_ids = Vec::new();
        let mut last_game_id = None;
        let mut has_more = false;

        let iter: Box<dyn Iterator<Item = (&u32, &u32)>> = match order {
            SortOrder::Asc => Box::new(entries.iter()),
            SortOrder::Desc => Box::new(entries.iter().rev()),
        };

        for (game_id, _) in iter {
            if let Some(cursor) = cursor {
                match order {
                    SortOrder::Asc if *game_id <= cursor => continue,
                    SortOrder::Desc if *game_id >= cursor => continue,
                    _ => {}
                }
            }

            if game_ids.len() < limit {
                game_ids.push(*game_id);
                last_game_id = Some(*game_id);
            } else {
                has_more = true;
                break;
            }
        }

        Ok(UnclaimedGameIdPage {
            game_ids,
            next_cursor: if has_more { last_game_id } else { None },
        })
    }

    fn mark_unclaimed(
        &mut self,
        account: &Identity,
        game_id: u32,
        height: u32,
    ) -> crate::Result<()> {
        let key = Self::identity_key(account);
        let mut guard = self.unclaimed_snapshots.lock().unwrap();
        let entry = guard.entry(key).or_default();
        entry.insert(game_id, height);
        Ok(())
    }

    fn clear_unclaimed(&mut self, account: &Identity, game_id: u32) -> crate::Result<()> {
        let key = Self::identity_key(account);
        let mut guard = self.unclaimed_snapshots.lock().unwrap();
        if let Some(inner) = guard.get_mut(&key) {
            inner.remove(&game_id);
        }
        Ok(())
    }

    fn update_snapshot(
        &mut self,
        snapshot: &OverviewSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        let mut guard = self.snapshot.lock().unwrap();
        self.latest_game_id = snapshot.game_id;
        *guard = Some((snapshot.clone(), height));
        Ok(())
    }

    fn update_account_snapshot(
        &mut self,
        account: &Identity,
        game_id: u32,
        account_snapshot: &AccountSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        let key = Self::identity_key(account);
        let mut guard = self.account_snapshots.lock().unwrap();
        let inner_map = guard.entry(key).or_default();
        inner_map.insert(game_id, (account_snapshot.clone(), height));
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
