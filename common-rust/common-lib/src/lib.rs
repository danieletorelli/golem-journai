pub mod database;
pub mod model;

use crate::model::{
    AnalyzerError, CollectorError, JournalEntry, ServiceErrors, ServiceErrorsNoEntries,
};
use golem_rust::agent_definition;

#[agent_definition]
pub trait Collector {
    fn new(hostname: String) -> Self;

    fn collect(&self, entries: Vec<JournalEntry>) -> Result<u64, CollectorError>;

    fn get_entries(
        &self,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> Result<(Vec<JournalEntry>, u64), CollectorError>;

    fn get_error_spikes(&self) -> Result<Vec<ServiceErrorsNoEntries>, CollectorError>;
}

#[agent_definition]
pub trait Analyzer {
    fn new(hostname: String, service: String) -> Self;

    fn analyze_spike(&self, entries: Vec<ServiceErrors>) -> Result<(), AnalyzerError>;
}
