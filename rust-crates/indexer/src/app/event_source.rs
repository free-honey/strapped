use crate::{Result, events::Event};

pub trait EventSource {
    fn next_event(&mut self) -> impl Future<Output = Result<(Event, u32)>>;
}
