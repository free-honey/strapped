use crate::{
    Result,
    events::Event,
};

pub trait EventSource {
    fn next_event_batch(
        &mut self,
    ) -> impl Future<Output = Result<Option<(Vec<Event>, u32)>>>;
}
