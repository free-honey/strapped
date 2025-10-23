use crate::{
    Result,
    app::event_source::EventSource,
    events::{
        Event,
        Roll as AppRoll,
    },
};
use anyhow::anyhow;
use fuel_core::{
    service::ServiceTrait,
    state::rocks_db::{
        ColumnsPolicy,
        DatabaseConfig,
    },
};
use fuel_core_services::{
    ServiceRunner,
    stream::BoxStream,
};
use fuel_indexer::{
    adapters::SimplerProcessorAdapter,
    fuel_events_manager,
    fuel_events_manager::{
        port::StorableEvent,
        service::UnstableEvent,
    },
    fuel_receipts_manager,
    indexer::Task,
    processors::simple_processor::FnReceiptParser,
    try_parse_events,
};
use fuels::{
    core::{
        codec::{
            DecoderConfig,
            Log,
        },
        traits::Tokenizable,
    },
    prelude::{
        AssetConfig,
        AssetId,
        ContractId,
        DbType,
        NodeConfig,
        Receipt,
        WalletsConfig,
    },
    types::Token,
};
use generated_abi::strapped_types::{
    InitializedEvent,
    Roll as AbiRoll,
    RollEvent as AbiRollEvent,
};
use std::convert::TryFrom;
use tokio_stream::StreamExt;

pub struct FuelIndexerEventSource<Fn>
where
    Fn: FnOnce(DecoderConfig, &Receipt) -> Option<Event> + Copy + Send + Sync + 'static,
{
    service: ServiceRunner<
        Task<
            SimplerProcessorAdapter<FnReceiptParser<Fn>>,
            fuel_receipts_manager::rocksdb::Storage,
            fuel_events_manager::rocksdb::Storage,
        >,
    >,
    stream: BoxStream<Result<UnstableEvent<Event>>>,
}

impl StorableEvent for Event {}

impl<Fn> EventSource for FuelIndexerEventSource<Fn>
where
    Fn: FnOnce(DecoderConfig, &Receipt) -> Option<Event> + Copy + Send + Sync + 'static,
{
    async fn next_event_batch(&mut self) -> Result<(Vec<Event>, u32)> {
        let unstable_event = self
            .stream
            .next()
            .await
            .ok_or(anyhow::anyhow!("no event"))?
            .map_err(|e| anyhow!("failed retrieving next events: {e:?}"))?;
        match unstable_event {
            UnstableEvent::Events((height, events)) => Ok((events, *height)),
            UnstableEvent::Checkpoint(_) => {
                // TODO: WE should accomodate this happening and handle gracefully
                Ok((vec![], 0))
            }
            UnstableEvent::Rollback(_) => {
                todo!()
            }
        }
    }
}

impl<Fn> FuelIndexerEventSource<Fn>
where
    Fn: FnOnce(DecoderConfig, &Receipt) -> Option<Event> + Copy + Send + Sync + 'static,
{
    pub async fn new(
        handler: Fn,
        temp_dir: std::path::PathBuf,
        database_config: DatabaseConfig,
        indexer_config: fuel_indexer::indexer::IndexerConfig,
    ) -> Result<Self> {
        let service = fuel_indexer::indexer::new_logs_indexer(
            handler,
            temp_dir,
            database_config,
            indexer_config,
        )?;
        service.start_and_await().await?;
        let stream = service.shared.events_starting_from(0u32.into()).await?;
        let new = Self { service, stream };
        Ok(new)
    }
}

fn map_roll(roll: AbiRoll) -> AppRoll {
    match roll {
        AbiRoll::Two => AppRoll::Two,
        AbiRoll::Three => AppRoll::Three,
        AbiRoll::Four => AppRoll::Four,
        AbiRoll::Five => AppRoll::Five,
        AbiRoll::Six => AppRoll::Six,
        AbiRoll::Seven => AppRoll::Seven,
        AbiRoll::Eight => AppRoll::Eight,
        AbiRoll::Nine => AppRoll::Nine,
        AbiRoll::Ten => AppRoll::Ten,
        AbiRoll::Eleven => AppRoll::Eleven,
        AbiRoll::Twelve => AppRoll::Twelve,
    }
}

fn parse_event_logs(decoder: DecoderConfig, receipt: &Receipt) -> Option<Event> {
    try_parse_events!(
        [decoder, receipt]
        InitializedEvent => |event| {
            tracing::info!("xxxxxxxxx");
            let inner = Event::init_event(
                ContractId::from(event.vrf_contract_id.0),
                AssetId::from(event.chip_asset_id),
                event.roll_frequency,
                event.first_height,
            );
            Some(inner)
        },
        AbiRollEvent => |event| {
            let game_id = u32::try_from(event.game_id).ok()?;
            let roll_index = u32::try_from(event.roll_index).ok()?;
            let rolled_value = map_roll(event.rolled_value);
            Some(Event::roll_event(game_id, roll_index, rolled_value))
        }

    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::init_tracing,
        events::ContractEvent,
    };
    use fuels::{
        prelude::{
            AssetConfig,
            AssetId,
            Contract,
            LoadConfiguration,
            TxPolicies,
            WalletsConfig,
            launch_custom_provider_and_get_wallets,
        },
        programs::calls::Execution,
        types::Bits256,
    };
    use generated_abi::{
        get_contract_instance,
        vrf_types::FakeVRFContract,
    };
    use url::Url;

    #[tokio::test]
    async fn next_event_batch__can_get_init_event() {
        init_tracing();

        let chip_asset_id = AssetId::new([1u8; 32]);
        let base_assets = vec![
            AssetConfig {
                id: AssetId::zeroed(),
                num_coins: 1,
                coin_amount: 10_000_000_000,
            },
            AssetConfig {
                id: chip_asset_id,
                num_coins: 1,
                coin_amount: 10_000_000_000,
            },
        ];
        let temp_dir = tempdir::TempDir::new("database")
            .unwrap()
            .path()
            .to_path_buf();

        let database_config = DatabaseConfig {
            cache_capacity: None,
            max_fds: 512,
            columns_policy: ColumnsPolicy::Lazy,
        };
        let mut wallets = launch_custom_provider_and_get_wallets(
            WalletsConfig::new_multiple_assets(1, base_assets),
            None,
            None,
        )
        .await
        .expect("failed to launch local provider");
        let wallet = wallets.pop().unwrap();

        let (contract_instance, _contract_id) =
            get_contract_instance(wallet.clone()).await;
        let address = wallet.provider().url();
        let indexer_config = fuel_indexer::indexer::IndexerConfig::new(
            0u32.into(),
            Url::parse(address).unwrap(),
        );

        let mut event_source = FuelIndexerEventSource::new(
            parse_event_logs,
            temp_dir,
            database_config,
            indexer_config,
        )
        .await
        .unwrap();

        // given
        let fake_vrf_contract_id = [5; 32];

        // when
        contract_instance
            .methods()
            .initialize(Bits256(fake_vrf_contract_id), chip_asset_id.clone(), 100)
            .call()
            .await
            .unwrap();

        // then
        let _should_be_empty_first_block = event_source.next_event_batch().await.unwrap();
        let _checkpoint = event_source.next_event_batch().await.unwrap();
        let (events, _) = event_source.next_event_batch().await.unwrap();
        let actual = events.first().unwrap();
        let expected =
            Event::init_event(fake_vrf_contract_id.into(), chip_asset_id, 100, 2);

        assert_eq!(actual, &expected);
    }

    #[tokio::test]
    async fn next_event_batch__can_get_roll_event() {
        init_tracing();

        let chip_asset_id = AssetId::new([1u8; 32]);
        let base_assets = vec![
            AssetConfig {
                id: AssetId::zeroed(),
                num_coins: 1,
                coin_amount: 10_000_000_000,
            },
            AssetConfig {
                id: chip_asset_id,
                num_coins: 1,
                coin_amount: 10_000_000_000,
            },
        ];
        let temp_dir = tempdir::TempDir::new("database")
            .unwrap()
            .path()
            .to_path_buf();

        let database_config = DatabaseConfig {
            cache_capacity: None,
            max_fds: 512,
            columns_policy: ColumnsPolicy::Lazy,
        };
        let mut wallets = launch_custom_provider_and_get_wallets(
            WalletsConfig::new_multiple_assets(1, base_assets),
            None,
            None,
        )
        .await
        .expect("failed to launch local provider");
        let wallet = wallets.pop().unwrap();

        let (contract_instance, _) = get_contract_instance(wallet.clone()).await;

        let vrf_bin_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../sway-projects/fake-vrf-contract/out/release/fake-vrf-contract.bin",
        );
        let vrf_contract =
            Contract::load_from(vrf_bin_path, LoadConfiguration::default())
                .expect("failed to load fake vrf contract");
        let deployment = vrf_contract
            .deploy(&wallet, TxPolicies::default())
            .await
            .expect("failed to deploy fake vrf contract");
        let vrf_contract_id = deployment.contract_id.clone();
        let vrf_instance = FakeVRFContract::new(vrf_contract_id.clone(), wallet.clone());

        let address = wallet.provider().url();
        let indexer_config = fuel_indexer::indexer::IndexerConfig::new(
            0u32.into(),
            Url::parse(address).unwrap(),
        );

        let mut event_source = FuelIndexerEventSource::new(
            parse_event_logs,
            temp_dir,
            database_config,
            indexer_config,
        )
        .await
        .unwrap();

        contract_instance
            .methods()
            .initialize(Bits256(*vrf_contract_id), chip_asset_id.clone(), 1)
            .call()
            .await
            .unwrap();

        let provider = wallet.provider();
        let next_roll_height = contract_instance
            .methods()
            .next_roll_height()
            .simulate(Execution::state_read_only())
            .await
            .unwrap()
            .value
            .expect("expected next roll height");
        let current_height = provider
            .latest_block_height()
            .await
            .expect("failed to read current block height");
        if next_roll_height > current_height {
            provider
                .produce_blocks(next_roll_height - current_height, None)
                .await
                .expect("failed to advance blocks");
        }

        let vrf_number = 0u64;
        vrf_instance
            .methods()
            .set_number(vrf_number)
            .call()
            .await
            .unwrap();

        contract_instance
            .methods()
            .roll_dice()
            .with_contracts(&[&vrf_instance])
            .call()
            .await
            .unwrap();

        let mut actual_event = None;
        for _ in 0..10 {
            let (events, _) = event_source.next_event_batch().await.unwrap();
            if let Some(event) = events.into_iter().find(|event| {
                matches!(event, Event::ContractEvent(ContractEvent::Roll(_)))
            }) {
                actual_event = Some(event);
                break;
            }
        }

        let actual_event = actual_event.expect("expected to receive roll event");
        let expected = Event::roll_event(0, 1, crate::events::Roll::Two);
        assert_eq!(actual_event, expected);
    }
}
