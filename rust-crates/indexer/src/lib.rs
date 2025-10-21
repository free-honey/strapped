pub mod app;

pub mod snapshot;

pub mod events;

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub enum Error {}
