use golem_rust::Schema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub struct JournalEntry {
    /// Unique identifier for the current system boot session
    pub boot_id: String,
    /// Hostname of the system where the log was generated
    pub hostname: String,
    /// Unique identifier of the machine that generated the log entry
    pub machine_id: String,
    /// Syslog priority level (0=emergency to 7=debug)
    pub priority: String,
    /// The actual log message content
    pub message: String,
    /// Timestamp of the journal entry as a floating-point Unix epoch time
    pub date: f64,
    /// Runtime scope of the logging process (e.g., "system" or "user")
    pub runtime_scope: String,
    /// Process ID (PID) that generated the log entry
    pub pid: Option<String>,
    /// User ID (UID) that owned the process generating the log entry
    pub uid: Option<String>,
    /// Group ID (GID) of the process that generated the log entry
    pub gid: Option<String>,
    /// Transport method used to receive the log (e.g., "syslog", "journal", "kernel")
    pub transport: Option<String>,
    /// Syslog facility code indicating the type of program logging the message
    pub syslog_facility: Option<String>,
    /// Identifier string of the program that sent the syslog message
    pub syslog_identifier: Option<String>,
    /// Process command name (short form of the executable name)
    pub comm: Option<String>,
    /// Full path to the executable file of the process
    pub exe: Option<String>,
    /// Complete command line used to invoke the process
    pub cmdline: Option<String>,
    /// Systemd unit name associated with the log entry
    pub unit: Option<String>,
    /// Systemd unit name that triggered or is associated with the log entry
    pub systemd_unit: Option<String>,
    /// Systemd slice to which the process belongs (hierarchical grouping)
    pub systemd_slice: Option<String>,
    /// Systemd control group (cgroup) path for resource management and process tracking
    pub systemd_cgroup: Option<String>,
    /// Line number in the source code file where the log was generated
    pub code_line: Option<String>,
    /// Source code file path where the log was generated
    pub code_file: Option<String>,
    /// Unique identifier for the systemd job
    pub job_id: Option<String>,
    /// Result status of the systemd job (e.g., "success", "failed")
    pub job_result: Option<String>,
    /// Type of systemd job (e.g., "start", "stop")
    pub job_type: Option<String>,
    /// Unique invocation identifier for the current service execution
    pub invocation_id: Option<String>,
    /// Monotonic timestamp from the source (in microseconds, never goes backwards)
    pub source_monotonic_timestamp: Option<String>,
    /// Timestamp relative to boot time when the message was generated (in microseconds)
    pub source_boottime_timestamp: Option<String>,
}

impl PartialEq for JournalEntry {
    fn eq(&self, other: &Self) -> bool {
        self.boot_id == other.boot_id
            && self.hostname == other.hostname
            && self.machine_id == other.machine_id
            && self.message == other.message
            && self.pid == other.pid
            && self.uid == other.uid
            && self.gid == other.gid
            && self.date == other.date
    }
}

impl Eq for JournalEntry {}

#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub enum CollectorError {
    InsertError(String),
    FetchError(String),
}
