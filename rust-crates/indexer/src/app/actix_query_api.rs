use crate::{
    Result,
    app::query_api::{
        Query,
        QueryAPI,
    },
    events::Strap,
    snapshot::{
        ALL_ROLLS,
        AccountRollBets,
        AccountSnapshot,
        HistoricalSnapshot,
        OverviewSnapshot,
    },
};
use actix_web::{
    App,
    HttpServer,
    dev::ServerHandle,
    error::{
        ErrorInternalServerError,
        PayloadError,
        UrlencodedError,
    },
    web,
};
use anyhow::{
    Context,
    anyhow,
};
use fuels::types::{
    Address,
    AssetId,
    Identity,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    mem,
    net::TcpListener,
    str::FromStr,
    thread::JoinHandle,
};
use tokio::sync::{
    mpsc,
    oneshot,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LatestSnapshotDto {
    snapshot: OverviewSnapshot,
    block_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LatestAccountSnapshotDto {
    snapshot: AccountSnapshot,
    block_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct HistoricalSnapshotDto {
    snapshot: HistoricalSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StrapMetadataDto {
    asset_id: AssetId,
    strap: Strap,
}

fn normalize_account_snapshot(snapshot: &mut AccountSnapshot) {
    if snapshot.per_roll_bets.len() == ALL_ROLLS.len() {
        return;
    }

    let mut existing = mem::take(&mut snapshot.per_roll_bets);
    let mut rebuilt = Vec::with_capacity(ALL_ROLLS.len());
    for roll in ALL_ROLLS {
        if let Some(pos) = existing.iter().position(|entry| entry.roll == roll) {
            rebuilt.push(existing.swap_remove(pos));
        } else {
            rebuilt.push(AccountRollBets {
                roll,
                bets: Vec::new(),
            });
        }
    }
    snapshot.per_roll_bets = rebuilt;
}

pub struct ActixQueryApi {
    receiver: mpsc::Receiver<Query>,
    base_url: String,
    server_handle: ServerHandle,
    server_thread: Option<JoinHandle<()>>,
}

impl ActixQueryApi {
    pub async fn new(port: Option<u16>) -> Result<Self> {
        let (sender, receiver) = mpsc::channel(16);

        let listener = TcpListener::bind(("0.0.0.0", port.unwrap_or(0)))
            .context("failed to bind HTTP listener for query API")?;
        let address = listener
            .local_addr()
            .context("failed to read listener address")?;
        let base_url = format!("http://{}", address);

        tracing::info!("query API listening on {}", base_url);

        let server_sender = sender.clone();
        let server = HttpServer::new(move || {
            let sender = server_sender.clone();
            // server_routes(sender)

            App::new()
                .app_data(web::Data::new(sender))
                .route("/snapshot/latest", web::get().to(handle_latest_snapshot))
                .route(
                    "/account/{identity}/{game_id}",
                    web::get().to(handle_historical_account_snapshot),
                )
                .route(
                    "/account/{identity}",
                    web::get().to(handle_account_snapshot),
                )
                .route(
                    "/historical/{game_id}",
                    web::get().to(handle_historical_snapshot),
                )
                .route("/straps", web::get().to(handle_all_known_straps))
        })
        .listen(listener)
        .context("failed to start Actix server")?
        .run();

        let server_handle = server.handle();
        let server_thread = std::thread::spawn(move || {
            let sys = actix_web::rt::System::new();
            let _ = sys.block_on(server);
        });

        Ok(Self {
            receiver,
            base_url,
            server_handle,
            server_thread: Some(server_thread),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl QueryAPI for ActixQueryApi {
    async fn query(&mut self) -> Result<Query> {
        self.receiver
            .recv()
            .await
            .ok_or_else(|| anyhow!("query server closed"))
    }
}

impl Drop for ActixQueryApi {
    fn drop(&mut self) {
        drop(self.server_handle.stop(true));
        if let Some(thread) = self.server_thread.take() {
            let _ = thread.join();
        }
    }
}

async fn handle_latest_snapshot(
    sender: web::Data<mpsc::Sender<Query>>,
) -> actix_web::Result<web::Json<LatestSnapshotDto>> {
    tracing::info!("received latest snapshot request");
    let (response_sender, response_receiver) = oneshot::channel();
    let query = Query::LatestSnapshot(response_sender);

    sender.get_ref().clone().send(query).await.map_err(|_| {
        ErrorInternalServerError("unable to forward latest snapshot query")
    })?;

    let (mut snapshot, block_height) = response_receiver
        .await
        .map_err(|_| ErrorInternalServerError("latest snapshot responder dropped"))?;

    snapshot.current_block_height = block_height;

    Ok(web::Json(LatestSnapshotDto {
        snapshot,
        block_height,
    }))
}

async fn handle_account_snapshot(
    sender: web::Data<mpsc::Sender<Query>>,
    account_identity: web::Path<String>,
) -> actix_web::Result<web::Json<Option<LatestAccountSnapshotDto>>> {
    tracing::info!("received account snapshot request");
    let (response_sender, response_receiver) = oneshot::channel();
    let inner = Address::from_str(&account_identity)
        .map_err(|_| UrlencodedError::Payload(PayloadError::EncodingCorrupted))?;
    let identity = Identity::Address(inner);
    let query = Query::latest_account_summary(identity, response_sender);

    sender.get_ref().clone().send(query).await.map_err(|_| {
        ErrorInternalServerError("unable to forward latest snapshot query")
    })?;

    if let Some((mut snapshot, block_height)) = response_receiver
        .await
        .map_err(|_| ErrorInternalServerError("latest snapshot responder dropped"))?
    {
        normalize_account_snapshot(&mut snapshot);
        Ok(web::Json(Some(LatestAccountSnapshotDto {
            snapshot,
            block_height,
        })))
    } else {
        Ok(web::Json(None))
    }
}

async fn handle_historical_account_snapshot(
    sender: web::Data<mpsc::Sender<Query>>,
    path: web::Path<(String, u32)>,
) -> actix_web::Result<web::Json<Option<LatestAccountSnapshotDto>>> {
    tracing::info!("received historical account snapshot request");
    let (identity_str, game_id) = path.into_inner();
    let (response_sender, response_receiver) = oneshot::channel();
    let inner = Address::from_str(&identity_str)
        .map_err(|_| UrlencodedError::Payload(PayloadError::EncodingCorrupted))?;
    let identity = Identity::Address(inner);
    let query = Query::historical_account_summary(identity, game_id, response_sender);

    sender.get_ref().clone().send(query).await.map_err(|_| {
        ErrorInternalServerError("unable to forward historical account snapshot query")
    })?;

    if let Some((mut snapshot, block_height)) = response_receiver.await.map_err(|_| {
        ErrorInternalServerError("historical account snapshot responder dropped")
    })? {
        normalize_account_snapshot(&mut snapshot);
        Ok(web::Json(Some(LatestAccountSnapshotDto {
            snapshot,
            block_height,
        })))
    } else {
        Ok(web::Json(None))
    }
}

async fn handle_historical_snapshot(
    sender: web::Data<mpsc::Sender<Query>>,
    game_id: web::Path<u32>,
) -> actix_web::Result<web::Json<Option<HistoricalSnapshotDto>>> {
    tracing::info!("received historical snapshot request for {}", game_id);
    let (response_sender, response_receiver) = oneshot::channel();
    let query = Query::historical_snapshot(*game_id, response_sender);

    sender.get_ref().clone().send(query).await.map_err(|_| {
        ErrorInternalServerError("unable to forward historical snapshot query")
    })?;

    if let Some(snapshot) = response_receiver
        .await
        .map_err(|_| ErrorInternalServerError("historical snapshot responder dropped"))?
    {
        Ok(web::Json(Some(HistoricalSnapshotDto { snapshot })))
    } else {
        Ok(web::Json(None))
    }
}

async fn handle_all_known_straps(
    sender: web::Data<mpsc::Sender<Query>>,
) -> actix_web::Result<web::Json<Vec<StrapMetadataDto>>> {
    tracing::info!("received all known strap metadata request");
    let (response_sender, response_receiver) = oneshot::channel();
    let query = Query::all_known_straps(response_sender);

    sender.get_ref().clone().send(query).await.map_err(|_| {
        ErrorInternalServerError("unable to forward all strap metadata query")
    })?;

    let response = response_receiver
        .await
        .map_err(|_| ErrorInternalServerError("all strap metadata responder dropped"))?;

    let body: Vec<StrapMetadataDto> = response
        .into_iter()
        .map(|(asset_id, strap)| StrapMetadataDto { asset_id, strap })
        .collect();

    Ok(web::Json(body))
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::query_api::{
            AccountSnapshotQuery,
            HistoricalAccountSnapshotQuery,
            HistoricalSnapshotQuery,
        },
        events::{
            Modifier,
            Roll,
            Strap,
            StrapKind,
        },
        snapshot::ActiveModifier,
    };

    #[tokio::test]
    async fn query__can_get_and_respond_to_latest_overview_snapshot() {
        // given
        let mut api = ActixQueryApi::new(None).await.unwrap();
        let client = reqwest::Client::new();
        let url = format!("{}/snapshot/latest", api.base_url());
        let expected_height = 42;
        let mut expected_snapshot = OverviewSnapshot::new();
        expected_snapshot.current_block_height = expected_height;
        let expected_response = LatestSnapshotDto {
            snapshot: expected_snapshot.clone(),
            block_height: expected_height,
        };

        let client_task = tokio::spawn(async move {
            let response = client.get(url).send().await.unwrap();
            response.json::<LatestSnapshotDto>().await.unwrap()
        });

        // when
        let query = api.query().await.unwrap();
        if let Query::LatestSnapshot(sender) = query {
            sender
                .send((OverviewSnapshot::new(), expected_height))
                .unwrap();
        } else {
            panic!("expected latest snapshot query got {:?}", query);
        }
        // then
        let response = client_task.await.unwrap();
        assert_eq!(response, expected_response);
    }

    #[tokio::test]
    async fn query__can_get_and_respond_to_latest_account_snapshot() {
        // given
        let mut api = ActixQueryApi::new(None).await.unwrap();
        let client = reqwest::Client::new();
        let expected_identity = Identity::default();
        let expected_identity_str = match expected_identity {
            Identity::Address(address) => address.to_string(),
            Identity::ContractId(contract) => contract.to_string(),
        };
        tracing::info!("expected identity: {expected_identity_str}");
        let url = format!("{}/account/{expected_identity_str}", api.base_url());
        let expected_snapshot = AccountSnapshot::default();
        let expected_height = 42;
        let expected_response = LatestAccountSnapshotDto {
            snapshot: expected_snapshot.clone(),
            block_height: expected_height,
        };

        let client_task = tokio::spawn(async move {
            tracing::info!("setting up client task");
            let res = client.get(url).send().await;
            tracing::info!("got result from client: {res:?}");
            let response = res.unwrap();
            let deserialized = response.json::<LatestAccountSnapshotDto>().await.unwrap();
            tracing::info!("got snapshot: {deserialized:?}");
            deserialized
        });

        // when
        let query = api.query().await.unwrap();

        if let Query::LatestAccountSnapshot(inner) = query {
            tracing::info!("Got query: {inner:?}");
            let AccountSnapshotQuery { identity, sender } = inner;
            assert_eq!(expected_identity, identity);
            sender
                .send(Some((expected_snapshot.clone(), expected_height)))
                .unwrap();
        } else {
            panic!("expected latest account snapshot query got {:?}", query);
        }

        // then
        let response = client_task.await.unwrap();
        assert_eq!(response, expected_response);
    }

    #[tokio::test]
    async fn query__can_get_historical_account_snapshot() {
        // given
        let mut api = ActixQueryApi::new(None).await.unwrap();
        let client = reqwest::Client::new();
        let expected_identity = Identity::default();
        let expected_identity_str = match &expected_identity {
            Identity::Address(address) => address.to_string(),
            Identity::ContractId(contract) => contract.to_string(),
        };
        let expected_game_id = 7u32;
        let url = format!(
            "{}/account/{expected_identity_str}/{expected_game_id}",
            api.base_url()
        );
        let expected_snapshot = AccountSnapshot::default();
        let expected_height = 1337;
        let expected_response = LatestAccountSnapshotDto {
            snapshot: expected_snapshot.clone(),
            block_height: expected_height,
        };

        let client_task = tokio::spawn(async move {
            let response = client.get(url).send().await.unwrap();
            response.json::<LatestAccountSnapshotDto>().await.unwrap()
        });

        // when
        let query = api.query().await.unwrap();

        if let Query::HistoricalAccountSnapshot(inner) = query {
            let HistoricalAccountSnapshotQuery {
                identity,
                game_id,
                sender,
            } = inner;
            assert_eq!(expected_identity, identity);
            assert_eq!(expected_game_id, game_id);
            sender
                .send(Some((expected_snapshot.clone(), expected_height)))
                .unwrap();
        } else {
            panic!("expected historical account snapshot query got {:?}", query);
        }

        // then
        let response = client_task.await.unwrap();
        assert_eq!(response, expected_response);
    }

    #[tokio::test]
    async fn query__can_get_historical_snapshot() {
        // given
        let mut api = ActixQueryApi::new(None).await.unwrap();
        let client = reqwest::Client::new();
        let expected_game_id = 7u32;
        let url = format!("{}/historical/{expected_game_id}", api.base_url());
        let expected_snapshot = HistoricalSnapshot::new(
            expected_game_id,
            vec![Roll::Six, Roll::Seven],
            vec![ActiveModifier::new(1, Modifier::Lucky, Roll::Six)],
        );
        let expected_response = HistoricalSnapshotDto {
            snapshot: expected_snapshot.clone(),
        };

        let client_task = tokio::spawn(async move {
            let response = client.get(url).send().await.unwrap();
            response
                .json::<Option<HistoricalSnapshotDto>>()
                .await
                .unwrap()
                .expect("expected historical snapshot response")
        });

        // when
        let query = api.query().await.unwrap();

        if let Query::HistoricalSnapshot(inner) = query {
            let HistoricalSnapshotQuery { game_id, sender } = inner;
            assert_eq!(expected_game_id, game_id);
            sender.send(Some(expected_snapshot.clone())).unwrap();
        } else {
            panic!("expected historical snapshot query got {:?}", query);
        }

        // then
        let response = client_task.await.unwrap();
        assert_eq!(response, expected_response);
    }

    #[tokio::test]
    async fn query__can_get_all_known_straps() {
        // given
        let mut api = ActixQueryApi::new(None).await.unwrap();
        let client = reqwest::Client::new();
        let url = format!("{}/straps", api.base_url());
        let expected = vec![
            (
                AssetId::from([1u8; 32]),
                Strap::new(1, StrapKind::Hat, Modifier::Lucky),
            ),
            (
                AssetId::from([2u8; 32]),
                Strap::new(2, StrapKind::Scarf, Modifier::Holy),
            ),
        ];
        let client_task = tokio::spawn(async move {
            let response = client.get(url).send().await.unwrap();
            response.json::<Vec<StrapMetadataDto>>().await.unwrap()
        });

        // when
        let query = api.query().await.unwrap();
        if let Query::AllKnownStraps(sender) = query {
            sender.send(expected.clone()).unwrap();
        } else {
            panic!("expected all known strap metadata query got {:?}", query);
        }

        // then
        let mut response = client_task.await.unwrap();
        response.sort_by_key(|entry| entry.asset_id);
        let mut expected_sorted: Vec<StrapMetadataDto> = expected
            .into_iter()
            .map(|(asset_id, strap)| StrapMetadataDto { asset_id, strap })
            .collect();
        expected_sorted.sort_by_key(|entry| entry.asset_id);
        assert_eq!(response, expected_sorted);
    }
}
