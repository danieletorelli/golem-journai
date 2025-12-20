mod model;

use golem_rust::{agent_definition, agent_implementation};
use model::JournalEntry;

#[agent_definition]
pub trait Collector {
    fn new(hostname: String) -> Self;

    fn collect(&mut self, entries: Vec<JournalEntry>) -> usize;

    fn get_entries(
        &self,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> (Vec<JournalEntry>, usize);
}

struct CollectorImpl {
    hostname: String,
    entries: Vec<JournalEntry>,
}

#[agent_implementation]
impl Collector for CollectorImpl {
    fn new(hostname: String) -> Self {
        Self {
            hostname,
            entries: Vec::new(),
        }
    }

    fn collect(&mut self, entries: Vec<JournalEntry>) -> usize {
        let mut accepted_count = 0;
        let mut rejected_count = 0;

        for entry in entries {
            if self.matches_filters(&entry) {
                self.entries.push(entry);
                accepted_count += 1;
            } else {
                rejected_count += 1;
            }
        }

        log::info!("Collected {} entries", accepted_count);
        if rejected_count > 0 {
            log::warn!("Rejected {} entries", rejected_count);
        }

        accepted_count
    }

    fn get_entries(
        &self,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> (Vec<JournalEntry>, usize) {
        self.log_query_params(since, priority, &message_contains);
        let entries: Vec<JournalEntry> = self
            .entries
            .iter()
            .filter(|e| self.matches_entry(e, since, priority, &message_contains))
            .cloned()
            .collect();
        let count = entries.len();
        (entries, count)
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

    fn matches_entry(
        &self,
        entry: &JournalEntry,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: &Option<String>,
    ) -> bool {
        since.map_or(true, |s| entry.date >= s)
            && priority.map_or(true, |priority| {
                entry
                    .priority
                    .parse::<u8>()
                    .map(|p| p <= priority)
                    .unwrap_or(false)
            })
            && message_contains
                .as_ref()
                .map_or(true, |m| entry.message.contains(m))
    }

    fn matches_filters(&self, entry: &JournalEntry) -> bool {
        entry.hostname == self.hostname
            && entry.priority.parse::<u8>().map_or(false, |p| p <= 7)
            && !entry.message.is_empty()
    }
}
