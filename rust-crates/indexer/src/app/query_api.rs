pub trait QueryAPI {
    fn query(&self) -> impl Future<Output = crate::Result<Query>>;
}

pub enum Query {}
