use crate::{
    Result,
    app::query_api::{
        Query,
        QueryAPI,
    },
    snapshot::OverviewSnapshot,
};
use actix_web::{
    dev::ServerHandle,
    error::ErrorInternalServerError,
    web,
    App,
    HttpServer,
};
use anyhow::{
    anyhow,
    Context,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    net::TcpListener,
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

pub struct ActixQueryApi {
    receiver: mpsc::Receiver<Query>,
    base_url: String,
    server_handle: ServerHandle,
    server_thread: Option<JoinHandle<()>>,
}

impl ActixQueryApi {
    pub async fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::channel(16);

        let listener = TcpListener::bind(("127.0.0.1", 0))
            .context("failed to bind HTTP listener for query API")?;
        let address = listener
            .local_addr()
            .context("failed to read listener address")?;
        let base_url = format!("http://{}", address);

        let server_sender = sender.clone();
        let server = HttpServer::new(move || {
            let sender = server_sender.clone();
            App::new()
                .app_data(web::Data::new(sender))
                .route("/snapshot/latest", web::get().to(handle_latest_snapshot))
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
    let (response_sender, response_receiver) = oneshot::channel();
    let query = Query::LatestSnapshot(response_sender);

    sender
        .get_ref()
        .clone()
        .send(query)
        .await
        .map_err(|_| ErrorInternalServerError("unable to forward latest snapshot query"))?;

    let (snapshot, block_height) = response_receiver
        .await
        .map_err(|_| ErrorInternalServerError("latest snapshot responder dropped"))?;

    Ok(web::Json(LatestSnapshotDto {
        snapshot,
        block_height,
    }))
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn query__can_get_and_respond_to_latest_snapshot() {
        // given
        let mut api = ActixQueryApi::new().await.unwrap();
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
        match query {
            Query::LatestSnapshot(sender) => {
                sender
                    .send((expected_snapshot.clone(), expected_height))
                    .unwrap();
            }
        }

        // then
        let response = client_task.await.unwrap();
        assert_eq!(response, expected_response);
    }
}
