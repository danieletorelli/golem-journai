use crate::database::extract;
use golem_rust::bindings::golem::rdbms::postgres;
use golem_rust::golem_ai::golem::llm::llm;
use golem_rust::Schema;
use serde::{Deserialize, Serialize};

pub type JournalEntryId = u64;

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

impl From<&postgres::DbRow> for JournalEntry {
    fn from(row: &postgres::DbRow) -> Self {
        let v = &row.values;
        JournalEntry {
            boot_id: extract(&v[0]),
            hostname: extract(&v[1]),
            machine_id: extract(&v[2]),
            priority: extract(&v[3]),
            message: extract(&v[4]),
            date: extract(&v[5]),
            runtime_scope: extract(&v[6]),
            pid: extract(&v[7]),
            uid: extract(&v[8]),
            gid: extract(&v[9]),
            transport: extract(&v[10]),
            syslog_facility: extract(&v[11]),
            syslog_identifier: extract(&v[12]),
            comm: extract(&v[13]),
            exe: extract(&v[14]),
            cmdline: extract(&v[15]),
            unit: extract(&v[16]),
            systemd_unit: extract(&v[17]),
            systemd_slice: extract(&v[18]),
            systemd_cgroup: extract(&v[19]),
            code_line: extract(&v[20]),
            code_file: extract(&v[21]),
            job_id: extract(&v[22]),
            job_result: extract(&v[23]),
            job_type: extract(&v[24]),
            invocation_id: extract(&v[25]),
            source_monotonic_timestamp: extract(&v[26]),
            source_boottime_timestamp: extract(&v[27]),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub enum APIError {
    InsertError(String),
    FetchError(String),
    LLMError(String),
    Other(String),
}

pub enum APIErrorType {
    Insert,
    Fetch,
    LLM,
}

impl APIErrorType {
    pub fn of_postgres(self, error: postgres::Error) -> APIError {
        let message = match &error {
            postgres::Error::ConnectionFailure(e) => format!("Connection failure: {}", e),
            postgres::Error::QueryExecutionFailure(e) => format!("Query execution failure: {}", e),
            postgres::Error::QueryParameterFailure(e) => format!("Query parameter failure: {}", e),
            postgres::Error::QueryResponseFailure(e) => format!("Query response failure: {}", e),
            postgres::Error::Other(e) => e.clone(),
        };

        match self {
            APIErrorType::Insert => APIError::InsertError(message),
            APIErrorType::Fetch => APIError::FetchError(message),
            _ => APIError::Other(message),
        }
    }

    pub fn of_llm(self, error: llm::Error) -> APIError {
        let message = format!("LLM error: {}", error);

        match self {
            APIErrorType::LLM => APIError::LLMError(message),
            _ => APIError::Other(message),
        }
    }

    pub fn of_string(self, message: String) -> APIError {
        match self {
            APIErrorType::Insert => APIError::InsertError(message),
            APIErrorType::Fetch => APIError::FetchError(message),
            _ => APIError::Other(message),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub struct ServiceErrors {
    pub hostname: String,
    pub service_name: String,
    pub error_count: u64,
    pub min_priority: u8,
    pub started_at: f64,
    pub last_at: f64,
    pub entries: Vec<JournalEntryId>,
}

pub struct ServiceErrorsBuilder<'a> {
    pub hostname: String,
    pub row: &'a postgres::DbRow,
}

impl From<ServiceErrorsBuilder<'_>> for ServiceErrors {
    fn from(builder: ServiceErrorsBuilder<'_>) -> Self {
        let v = &builder.row.values;
        ServiceErrors {
            hostname: builder.hostname,
            service_name: extract(&v[0]),
            error_count: extract(&v[1]),
            min_priority: extract(&v[2]),
            started_at: extract(&v[3]),
            last_at: extract(&v[4]),
            entries: extract(&v[5]),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub struct ServiceErrorsNoEntries {
    pub hostname: String,
    pub service_name: String,
    pub error_count: u64,
    pub started_at: f64,
    pub last_at: f64,
}

impl From<ServiceErrors> for ServiceErrorsNoEntries {
    fn from(value: ServiceErrors) -> Self {
        ServiceErrorsNoEntries {
            hostname: value.hostname,
            service_name: value.service_name,
            error_count: value.error_count,
            started_at: value.started_at,
            last_at: value.last_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SpikeEventSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for SpikeEventSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = match self {
            SpikeEventSeverity::Low => "Low",
            SpikeEventSeverity::Medium => "Medium",
            SpikeEventSeverity::High => "High",
            SpikeEventSeverity::Critical => "Critical",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpikeEventAssertion {
    pub severity: SpikeEventSeverity,
    pub needs_user_action: bool,
}
