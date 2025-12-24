mod database;

use common_lib::database::PostgresDatabase;
use common_lib::model::*;
use common_lib::*;
use database::Database;
use golem_rust::agent_implementation;
use std::time::{SystemTime, UNIX_EPOCH};

struct CollectorImpl {
    hostname: String,
}

#[agent_implementation]
impl Collector for CollectorImpl {
    fn new(hostname: String) -> Self {
        Self { hostname }
    }

    fn collect(&self, entries: Vec<JournalEntry>) -> Result<u64, CollectorError> {
        let mut accepted_count: u64 = 0;
        let mut rejected_count: u64 = 0;
        let mut accepted_entries: Vec<JournalEntry> = Vec::new();

        for entry in entries {
            if self.matches_filters(&entry) {
                accepted_entries.push(entry);
                accepted_count += 1;
            } else {
                rejected_count += 1;
            }
        }

        PostgresDatabase::insert_entries(accepted_entries).map(|inserted_count| {
            log::info!("Collected {} entries", accepted_count);
            if rejected_count > 0 {
                log::warn!("Rejected {} entries", rejected_count);
            }
            if inserted_count != accepted_count {
                log::warn!(
                    "Inserted {} entries (accepted: {}",
                    inserted_count,
                    accepted_count
                );
            }

            accepted_count
        })
    }

    fn get_entries(
        &self,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> Result<(Vec<JournalEntry>, u64), CollectorError> {
        self.log_query_params(since, priority, &message_contains);
        PostgresDatabase::get_entries(self.hostname.to_string(), since, priority, message_contains)
            .map(|entries| {
                let count = entries.len() as u64;
                (entries, count)
            })
    }

    fn get_error_spikes(&self) -> Result<Vec<ServiceErrorsNoEntries>, CollectorError> {
        let last_analysis_timestamp =
            PostgresDatabase::get_last_analysis_timestamp(self.hostname.to_string())?;

        let since = last_analysis_timestamp.unwrap_or_else(|| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            now - (14.0 * 24.0 * 60.0 * 60.0) // Last two-week timestamp
        });

        let error_spikes = PostgresDatabase::get_error_spikes(self.hostname.to_string(), since);

        error_spikes.map(|errors| errors.into_iter().map(|e| e.into()).collect())
    }
}

impl CollectorImpl {
    fn log_query_params(
        &self,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: &Option<String>,
    ) {
        log::debug!(
            "Query for {} (since: {}, priority: {}, message_contains: {})",
            self.hostname,
            since.map_or_else(|| "any".to_string(), |x| x.to_string()),
            priority.map_or_else(|| "any".to_string(), |x| x.to_string()),
            message_contains.as_deref().unwrap_or("any")
        );
    }

    fn matches_filters(&self, entry: &JournalEntry) -> bool {
        entry.hostname == self.hostname
            && entry.priority.parse::<u8>().is_ok_and(|p| p <= 7)
            && !entry.message.is_empty()
    }
}
