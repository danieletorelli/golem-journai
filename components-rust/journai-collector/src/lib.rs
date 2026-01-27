mod database;

use common_lib::model::*;
use common_lib::*;
use golem_rust::agent_implementation;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

struct CollectorImpl {
    hostname: String,
}

#[agent_implementation]
impl Collector for CollectorImpl {
    fn new(hostname: String) -> Self {
        Self { hostname }
    }

    fn collect(&self, entries: Vec<JournalEntry>) -> Result<u64, APIError> {
        let (accepted_entries, rejected): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|entry| self.matches_filters(entry));

        let accepted_count = accepted_entries.len() as u64;
        let rejected_count = rejected.len() as u64;

        database::insert_entries(accepted_entries).map(|inserted_count| {
            log::info!("Collected {} entries", accepted_count);
            if rejected_count > 0 {
                log::warn!("Rejected {} entries", rejected_count);
            }
            if inserted_count != accepted_count {
                log::warn!(
                    "Inserted {} entries (accepted: {})",
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
    ) -> Result<(Vec<JournalEntry>, u64), APIError> {
        self.log_query_params(since, priority, &message_contains);
        database::get_entries(self.hostname.clone(), since, priority, message_contains).map(
            |entries| {
                let count = entries.len() as u64;
                (entries, count)
            },
        )
    }

    fn get_error_spikes(&self) -> Result<Vec<ServiceErrorsNoEntries>, APIError> {
        let hostname = self.hostname.clone();
        let model = env::var("JOURNAI_LLM_MODEL").map_err(|_| {
            APIErrorType::LLM.of_string("JOURNAI_LLM_MODEL env variable is not defined".to_string())
        })?;
        let since = database::get_last_analysis_timestamp(hostname.clone(), model.clone())?
            .unwrap_or_else(|| self.get_default_since_timestamp());

        database::get_error_spikes(hostname, since).map(|spikes| {
            spikes
                .into_iter()
                .map(|spike| {
                    log::debug!("Processing error spike for service: {}", spike.service_name);

                    AnalyzerClient::new_phantom(spike.hostname.clone(), spike.service_name.clone())
                        .trigger_analyze_spike(spike.clone());

                    ServiceErrorsNoEntries::from(spike)
                })
                .collect()
        })
    }
}

impl CollectorImpl {
    const ANALYSIS_WINDOW_DAYS: u8 = 14;
    const SECONDS_PER_DAY: u64 = 86400;

    fn get_default_since_timestamp(&self) -> f64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs_f64();

        let window_seconds = (Self::ANALYSIS_WINDOW_DAYS as u64 * Self::SECONDS_PER_DAY) as f64;
        now - window_seconds
    }

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
            && !entry.message.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(hostname: &str, priority: &str, message: &str) -> JournalEntry {
        JournalEntry {
            boot_id: "boot".to_string(),
            hostname: hostname.to_string(),
            machine_id: "machine".to_string(),
            priority: priority.to_string(),
            message: message.to_string(),
            date: 1.0,
            runtime_scope: "system".to_string(),
            pid: None,
            uid: None,
            gid: None,
            transport: None,
            syslog_facility: None,
            syslog_identifier: None,
            comm: None,
            exe: None,
            cmdline: None,
            unit: None,
            systemd_unit: None,
            systemd_slice: None,
            systemd_cgroup: None,
            code_line: None,
            code_file: None,
            job_id: None,
            job_result: None,
            job_type: None,
            invocation_id: None,
            source_monotonic_timestamp: None,
            source_boottime_timestamp: None,
        }
    }

    #[test]
    fn matches_filters_accepts_valid_entries() {
        // Verify valid entries pass filters
        let collector = CollectorImpl::new("host-a".to_string());
        let entry = sample_entry("host-a", "3", "ok");

        assert!(collector.matches_filters(&entry));
    }

    #[test]
    fn matches_filters_rejects_invalid_entries() {
        // Verify invalid host, priority, or message is rejected
        let collector = CollectorImpl::new("host-a".to_string());

        let wrong_host = sample_entry("host-b", "3", "ok");
        assert!(!collector.matches_filters(&wrong_host));

        let invalid_priority = sample_entry("host-a", "9", "ok");
        assert!(!collector.matches_filters(&invalid_priority));

        let empty_message = sample_entry("host-a", "3", "   ");
        assert!(!collector.matches_filters(&empty_message));
    }

    #[test]
    fn default_since_timestamp_respects_window() {
        // Verify default since timestamp uses the expected window
        let collector = CollectorImpl::new("host-a".to_string());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let since = collector.get_default_since_timestamp();
        let window =
            (CollectorImpl::ANALYSIS_WINDOW_DAYS as u64 * CollectorImpl::SECONDS_PER_DAY) as f64;

        assert!(since <= now);
        assert!(since >= now - window - 1.0);
    }
}
