use indexer::app::actix_query_api::ActixQueryApi;
use indexer::app::App;
use indexer::app::fuel_indexer_event_source::FuelIndexerEventSource;

fn main() {
    let events = FuelIndexerEventSource;
    let api = ActixQueryApi::new();
    let snapshots =
    let app = App::new();

}
