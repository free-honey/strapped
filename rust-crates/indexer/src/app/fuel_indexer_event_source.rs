use crate::{
    Result,
    app::event_source::EventSource,
    events::Event,
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
use generated_abi::strapped_types::InitializedEvent;
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

// use fuel_core::{
//     service::{
//         Config,
//         FuelService,
//         ServiceTrait,
//     },
//     state::rocks_db::{
//         ColumnsPolicy,
//         DatabaseConfig,
//     },
// };
// use fuel_indexer::{
//     fuel_events_manager::port::StorableEvent,
//     indexer::IndexerConfig,
//     try_parse_events,
// };
// use fuels::{
//     core::codec::DecoderConfig,
//     tx::Receipt,
// };
// use url::Url;
//
// #[tokio::test]
// async fn defining_logs_indexer() {
//     fuels::prelude::abigen!(Contract(
//         name = "OrderBook",
//         abi = "crates/indexer/processors/receipt_parser/order-book-abi.json"
//     ));
//
//     #[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
//     enum Event {
//         Created { timestamp: u64 },
//         Matched { timestamp: u64 },
//     }
//
//     impl StorableEvent for Event {}
//
//     fn parse_o2_logs(decoder: DecoderConfig, receipt: &Receipt) -> Option<Event> {
//         try_parse_events!(
//             [decoder, receipt]
//             OrderCreatedEvent => |event| {
//                 Some(Event::Created {
//                     timestamp: event.timestamp.unix,
//                 })
//             },
//             OrderMatchedEvent => |event| {
//                 Some(Event::Matched{
//                     timestamp: event.timestamp.unix,
//                 })
//             }
//         )
//     }
//
//     let node = FuelService::new_node(Config::local_node()).await.unwrap();
//     let url = Url::parse(format!("http://{}", node.bound_address).as_str()).unwrap();
//     let temp_dir = tempdir::TempDir::new("database").unwrap();
//     let database_config = DatabaseConfig {
//         cache_capacity: None,
//         max_fds: 512,
//         columns_policy: ColumnsPolicy::Lazy,
//     };
//
//     // Given
//     let indexer = fuel_indexer::indexer::new_logs_indexer(
//         parse_o2_logs,
//         temp_dir.path().to_path_buf(),
//         database_config,
//         IndexerConfig::new(0u32.into(), url),
//     )
//     .unwrap();
//     indexer.start_and_await().await.unwrap();
//
//     // When
//     let result = indexer.shared.events_starting_from(0u32.into()).await;
//
//     // Then
//     assert!(result.is_ok());
// }
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
            ContractId,
            DbType,
            NodeConfig,
            WalletsConfig,
            launch_custom_provider_and_get_wallets,
        },
        types::Bits256,
    };
    use generated_abi::get_contract_instance;
    use url::Url;

    #[tokio::test]
    async fn next_event__can_get_init_event() {
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

        let (contract_instance, contract_id) =
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
        // a fuel indexer event source, and
        // a node with contract deployed
        let fake_vrf_contract_id = [5; 32];

        // when
        // call the init contract method
        contract_instance
            .methods()
            .initialize(Bits256(fake_vrf_contract_id), chip_asset_id.clone(), 100)
            .call()
            .await
            .unwrap();

        // then
        // received an init event
        let _should_be_empty_first_block = event_source.next_event_batch().await.unwrap();
        let _checkpoint = event_source.next_event_batch().await.unwrap();
        let (events, _) = event_source.next_event_batch().await.unwrap();
        let actual = events.first().unwrap();
        let expected =
            Event::init_event(fake_vrf_contract_id.into(), chip_asset_id, 100, 2);

        assert_eq!(actual, &expected);
    }
}
