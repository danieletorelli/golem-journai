pub mod database;
pub mod model;

use crate::model::{
    APIError, CollectResponse, EntriesResponse, ErrorSpikesResponse, JournalEntry, ServiceErrors,
};
use golem_rust::{agent_definition, endpoint};

#[agent_definition(ephemeral, mount = "/", cors = ["*"])]
pub trait Collector {
    fn new() -> Self;

    #[endpoint(post = "/collect/{hostname}")]
    fn collect(
        &self,
        hostname: String,
        entries: Vec<JournalEntry>,
    ) -> Result<CollectResponse, APIError>;

    #[endpoint(
        get = "/entries/{hostname}?since={since}&priority={priority}&contains={message_contains}"
    )]
    fn get_entries(
        &self,
        hostname: String,
        since: Option<f64>,
        priority: Option<i32>,
        message_contains: Option<String>,
    ) -> Result<EntriesResponse, APIError>;

    #[endpoint(get = "/errors/{hostname}")]
    fn get_error_spikes(&self, hostname: String) -> Result<ErrorSpikesResponse, APIError>;
}

#[agent_definition]
pub trait Analyzer {
    fn new(hostname: String, service: String) -> Self;

    async fn analyze_spike(&mut self, errors: ServiceErrors) -> Result<String, APIError>;
}

#[agent_definition(ephemeral, mount = "/", cors = ["*"])]
pub trait Visualizer {
    fn new() -> Self;

    #[endpoint(get = "/dashboard/overview")]
    fn dashboard_overview(&self) -> Result<String, APIError>;
    #[endpoint(get = "/dashboard/alerts")]
    fn dashboard_alerts(&self) -> Result<String, APIError>;
    #[endpoint(get = "/analysis/queue")]
    fn analysis_queue(&self) -> Result<String, APIError>;
    #[endpoint(get = "/analysis/history/{hostname}")]
    fn analysis_history(&self, hostname: String) -> Result<String, APIError>;
    #[endpoint(get = "/analysis/details/{analysis_id}")]
    fn analysis_details(&self, analysis_id: String) -> Result<String, APIError>;
}
