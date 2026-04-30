mod database;

use common_lib::model::*;
use common_lib::*;
use golem_rust::agent_implementation;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

struct CollectorImpl;

#[agent_implementation]
impl Collector for CollectorImpl {
    fn new() -> Self {
        Self
    }

    fn collect(
        &self,
        hostname: String,
        entries: Vec<JournalEntry>,
    ) -> Result<CollectResponse, APIError> {
        let (accepted_entries, rejected): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|entry| Self::matches_filters(&hostname, entry));

        let accepted_count = accepted_entries.len() as u64;
        let rejected_count = rejected.len() as u64;

        database::insert_entries(accepted_entries).map(move |inserted_count| {
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

            CollectResponse {
                success: true,
                message: format!("Collected {} entries", accepted_count),
            }
        })
    }

    fn get_entries(
        &self,
        hostname: String,
        since: Option<f64>,
        priority: Option<i32>,
        message_contains: Option<String>,
    ) -> Result<EntriesResponse, APIError> {
        self.log_query_params(&hostname, since, priority, &message_contains);
        let contains = message_contains.clone();
        let normalized_priority = match priority {
            Some(value) if value < 0 => None,
            Some(value) => u8::try_from(value).map(Some).map_err(|_| {
                APIErrorType::Fetch.of_string(format!(
                    "Invalid priority filter '{}': expected -1 or 0..255",
                    value
                ))
            })?,
            None => None,
        };

        database::get_entries(hostname, since, normalized_priority, message_contains).map(
            |entries| {
                let count = entries.len() as u64;
                EntriesResponse {
                    success: true,
                    results: Some(EntriesResult { entries, count }),
                    filters: EntriesFilters {
                        since,
                        priority: normalized_priority,
                        contains,
                    },
                    error: None,
                }
            },
        )
    }

    fn get_error_spikes(&self, hostname: String) -> Result<ErrorSpikesResponse, APIError> {
        let model = env::var("JOURNAI_LLM_MODEL").map_err(|_| {
            APIErrorType::LLM.of_string("JOURNAI_LLM_MODEL env variable is not defined".to_string())
        })?;
        let since = database::get_last_analysis_timestamp(hostname.clone(), model.clone())?
            .unwrap_or_else(Self::get_default_since_timestamp);

        database::get_error_spikes(hostname, since).map(|spikes| {
            let results = spikes
                .into_iter()
                .map(|spike| {
                    log::debug!("Processing error spike for service: {}", spike.service_name);

                    AnalyzerClient::new_phantom(spike.hostname.clone(), spike.service_name.clone())
                        .trigger_analyze_spike(spike.clone());

                    ServiceErrorsNoEntries::from(spike)
                })
                .collect();

            ErrorSpikesResponse {
                success: true,
                results: Some(results),
                error: None,
            }
        })
    }
}

impl CollectorImpl {
    const ANALYSIS_WINDOW_DAYS: u8 = 14;
    const SECONDS_PER_DAY: u64 = 86400;

    fn get_default_since_timestamp() -> f64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs_f64();

        let window_seconds = (Self::ANALYSIS_WINDOW_DAYS as u64 * Self::SECONDS_PER_DAY) as f64;
        now - window_seconds
    }

    fn log_query_params(
        &self,
        hostname: &str,
        since: Option<f64>,
        priority: Option<i32>,
        message_contains: &Option<String>,
    ) {
        log::debug!(
            "Query for {} (since: {}, priority: {}, message_contains: {})",
            hostname,
            since.map_or_else(|| "any".to_string(), |x| x.to_string()),
            priority.map_or_else(|| "any".to_string(), |x| x.to_string()),
            message_contains.as_deref().unwrap_or("any")
        );
    }

    fn matches_filters(hostname: &str, entry: &JournalEntry) -> bool {
        entry.hostname == hostname
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
        let entry = sample_entry("host-a", "3", "ok");

        assert!(CollectorImpl::matches_filters("host-a", &entry));
    }

    #[test]
    fn matches_filters_accepts_priority_boundaries() {
        // Verify priority bounds are inclusive
        let min_priority = sample_entry("host-a", "0", "ok");
        let max_priority = sample_entry("host-a", "7", "ok");

        assert!(CollectorImpl::matches_filters("host-a", &min_priority));
        assert!(CollectorImpl::matches_filters("host-a", &max_priority));
    }

    #[test]
    fn matches_filters_rejects_invalid_entries() {
        // Verify invalid host, priority, or message is rejected
        let wrong_host = sample_entry("host-b", "3", "ok");
        assert!(!CollectorImpl::matches_filters("host-a", &wrong_host));

        let invalid_priority = sample_entry("host-a", "9", "ok");
        assert!(!CollectorImpl::matches_filters("host-a", &invalid_priority));

        let non_numeric_priority = sample_entry("host-a", "nope", "ok");
        assert!(!CollectorImpl::matches_filters(
            "host-a",
            &non_numeric_priority
        ));

        let empty_message = sample_entry("host-a", "3", "   ");
        assert!(!CollectorImpl::matches_filters("host-a", &empty_message));
    }

    #[test]
    fn default_since_timestamp_respects_window() {
        // Verify default since timestamp uses the expected window
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let since = CollectorImpl::get_default_since_timestamp();
        let window =
            (CollectorImpl::ANALYSIS_WINDOW_DAYS as u64 * CollectorImpl::SECONDS_PER_DAY) as f64;

        assert!(since <= now);
        assert!(since >= now - window - 1.0);
    }
}
