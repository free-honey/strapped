use crate::{
    Result,
    app::query_api::{
        Query,
        QueryAPI,
    },
};

pub struct ReqwestQueryApi {}

impl QueryAPI for ReqwestQueryApi {
    async fn query(&mut self) -> Result<Query> {
        todo!()
    }
}
