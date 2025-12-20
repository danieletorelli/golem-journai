mod model;

use golem_rust::{agent_definition, agent_implementation};
use model::JournalEntry;

#[agent_definition]
pub trait Collector {
    fn new(hostname: String) -> Self;

    fn collect(&mut self, entries: Vec<JournalEntry>) -> usize;

    fn get_entries(&self) -> Vec<JournalEntry>;

    fn get_entries_count(&self) -> usize;
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
        log::warn!("Rejected {} entries", rejected_count);

        accepted_count
    }

    fn get_entries(&self) -> Vec<JournalEntry> {
        log::debug!("Getting entries: {:?}", self.entries);
        self.entries.clone()
    }

    fn get_entries_count(&self) -> usize {
        log::debug!("Getting entries count: {}", self.entries.len());
        self.entries.len()
    }
}

impl CollectorImpl {
    fn matches_filters(&self, entry: &JournalEntry) -> bool {
        entry.hostname == self.hostname //&& entry.systemd_unit == self.unit
    }
}
