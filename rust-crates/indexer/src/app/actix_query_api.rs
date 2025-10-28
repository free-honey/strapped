use crate::{
    Result,
    app::query_api::{
        Query,
        QueryAPI,
    },
    snapshot::{
        AccountSnapshot,
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
    Identity,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
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

pub struct ActixQueryApi {
    receiver: mpsc::Receiver<Query>,
    base_url: String,
    server_handle: ServerHandle,
    server_thread: Option<JoinHandle<()>>,
}

impl ActixQueryApi {
    pub async fn new(port: Option<u16>) -> Result<Self> {
        let (sender, receiver) = mpsc::channel(16);

        let listener = TcpListener::bind(("127.0.0.1", port.unwrap_or(0)))
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
                    "/account/{identity}",
                    web::get().to(handle_account_snapshot),
                )
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
        let _ = self.server_handle.stop(true);
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

    let (snapshot, block_height) = response_receiver
        .await
        .map_err(|_| ErrorInternalServerError("latest snapshot responder dropped"))?;

    Ok(web::Json(LatestSnapshotDto {
        snapshot,
        block_height,
    }))
}

async fn handle_account_snapshot(
    sender: web::Data<mpsc::Sender<Query>>,
    account_identity: web::Path<String>,
) -> actix_web::Result<web::Json<LatestAccountSnapshotDto>> {
    tracing::info!("received account snapshot request");
    let (response_sender, response_receiver) = oneshot::channel();
    let inner = Address::from_str(&account_identity)
        .map_err(|_| UrlencodedError::Payload(PayloadError::EncodingCorrupted))?;
    let identity = Identity::Address(inner);
    let query = Query::latest_account_summary(identity, response_sender);

    sender.get_ref().clone().send(query).await.map_err(|_| {
        ErrorInternalServerError("unable to forward latest snapshot query")
    })?;

    let (snapshot, block_height) = response_receiver
        .await
        .map_err(|_| ErrorInternalServerError("latest snapshot responder dropped"))?;

    Ok(web::Json(LatestAccountSnapshotDto {
        snapshot,
        block_height,
    }))
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::query_api::AccountSnapshotQuery;

    #[tokio::test]
    async fn query__can_get_and_respond_to_latest_overview_snapshot() {
        // given
        let mut api = ActixQueryApi::new(None).await.unwrap();
        let client = reqwest::Client::new();
        let url = format!("{}/snapshot/latest", api.base_url());
        let expected_snapshot = OverviewSnapshot::new();
        let expected_height = 42;
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
                .send((expected_snapshot.clone(), expected_height))
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
                .send((expected_snapshot.clone(), expected_height))
                .unwrap();
        } else {
            panic!("expected latest account snapshot query got {:?}", query);
        }

        // then
        let response = client_task.await.unwrap();
        assert_eq!(response, expected_response);
    }
}
