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

#[agent_definition]
pub trait Visualizer {
    fn new() -> Self;

    fn dashboard_overview(&self) -> Result<String, APIError>;
    fn dashboard_alerts(&self) -> Result<String, APIError>;
    fn analysis_queue(&self) -> Result<String, APIError>;
    fn analysis_history(&self, hostname: String) -> Result<String, APIError>;
    fn analysis_details(&self, analysis_id: String) -> Result<String, APIError>;
}
