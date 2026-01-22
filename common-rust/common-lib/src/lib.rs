pub mod database;
pub mod model;

use crate::model::{APIError, JournalEntry, ServiceErrors, ServiceErrorsNoEntries};
use golem_rust::agent_definition;

#[agent_definition(ephemeral)]
pub trait Collector {
    fn new(hostname: String) -> Self;

    fn collect(&self, entries: Vec<JournalEntry>) -> Result<u64, APIError>;

    fn get_entries(
        &self,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> Result<(Vec<JournalEntry>, u64), APIError>;

    fn get_error_spikes(&self) -> Result<Vec<ServiceErrorsNoEntries>, APIError>;
}

#[agent_definition]
pub trait Analyzer {
    fn new(hostname: String, service: String) -> Self;

    async fn analyze_spike(&mut self, errors: ServiceErrors) -> Result<String, APIError>;
}

#[agent_definition(ephemeral)]
pub trait Visualizer {
    fn new(hostname: String) -> Self;
}
