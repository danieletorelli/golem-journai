mod database;

use common_lib::model::{AnalyzerError, ServiceErrors};
use common_lib::Analyzer;
use golem_rust::agent_implementation;

struct AnalyzerImpl {
    hostname: String,
    service: String,
}

#[agent_implementation(ephemeral)]
impl Analyzer for AnalyzerImpl {
    fn new(hostname: String, service: String) -> Self {
        Self { hostname, service }
    }

    fn analyze_spike(&self, entries: Vec<ServiceErrors>) -> Result<(), AnalyzerError> {
        todo!()
    }
}
