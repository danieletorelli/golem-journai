use common_lib::database::{extract, should_log_queries};
use common_lib::model::{
    APIError, APIErrorType, JournalEntry, ServiceErrors, ServiceErrorsNoEntries, SpikeEventSeverity,
};
use golem_rust::bindings::golem::rdbms::postgres::{DbValue, Error};
use golem_rust::bindings::golem::rdbms::types::{Date, Time, Timestamp, Timestamptz};
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn sample_entry(priority: &str, message: &str) -> JournalEntry {
    JournalEntry {
        boot_id: "boot".to_string(),
        hostname: "host".to_string(),
        machine_id: "machine".to_string(),
        priority: priority.to_string(),
        message: message.to_string(),
        date: 123.0,
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
fn should_log_queries_reads_env_flag() {
    // Verify env flag toggles query logging
    let _guard = env_lock().lock().unwrap();

    std::env::remove_var("DATABASE_QUERY_LOG");
    assert!(!should_log_queries());

    std::env::set_var("DATABASE_QUERY_LOG", "true");
    assert!(should_log_queries());

    std::env::set_var("DATABASE_QUERY_LOG", "false");
    assert!(!should_log_queries());

    std::env::remove_var("DATABASE_QUERY_LOG");
}

#[test]
fn extract_converts_basic_types() {
    // Verify DbValue types map to target primitives
    assert_eq!(
        extract::<String>(&DbValue::Text("hello".to_string())),
        "hello"
    );
    assert_eq!(extract::<String>(&DbValue::Int4(42)), "42");
    assert_eq!(extract::<i16>(&DbValue::Int4(12)), 12);
    assert_eq!(extract::<i32>(&DbValue::Int8(99)), 99);
    assert_eq!(extract::<i64>(&DbValue::Int2(7)), 7);
    assert_eq!(extract::<u64>(&DbValue::Int8(10)), 10);
    assert_eq!(extract::<u8>(&DbValue::Int2(3)), 3);
    assert!(extract::<bool>(&DbValue::Boolean(true)));
    assert!(!extract::<bool>(&DbValue::Boolean(false)));
}

#[test]
fn extract_handles_null_and_boolean_fallbacks() {
    // Verify NULLs and numeric fallbacks map to expected booleans.
    assert!(!extract::<bool>(&DbValue::Null));
    assert!(extract::<bool>(&DbValue::Int2(1)));
    assert!(!extract::<bool>(&DbValue::Int2(0)));
    assert!(extract::<bool>(&DbValue::Int8(2)));
    assert!(!extract::<bool>(&DbValue::Float8(0.0)));
    assert!(extract::<bool>(&DbValue::Float8(0.1)));
    assert!(extract::<Option<String>>(&DbValue::Null).is_none());
}

#[test]
fn extract_formats_timestamp_types() {
    // Verify timestamp values format as expected strings
    let date = Date {
        year: 2024,
        month: 1,
        day: 2,
    };
    let time = Time {
        hour: 3,
        minute: 4,
        second: 5,
        nanosecond: 0,
    };
    let timestamp = Timestamp { date, time };

    assert_eq!(
        extract::<String>(&DbValue::Timestamp(timestamp)),
        "2024-01-02 03:04:05"
    );

    let date = Date {
        year: 2024,
        month: 1,
        day: 2,
    };
    let time = Time {
        hour: 3,
        minute: 4,
        second: 5,
        nanosecond: 0,
    };
    let timestamptz = Timestamptz {
        timestamp: Timestamp { date, time },
        offset: 0,
    };

    assert_eq!(
        extract::<String>(&DbValue::Timestamptz(timestamptz)),
        "2024-01-02 03:04:05 UTC"
    );
}

#[test]
fn extract_timestamp_as_epoch_seconds() {
    // Verify timestamp conversion to epoch seconds
    let timestamp = Timestamp {
        date: Date {
            year: 1970,
            month: 1,
            day: 1,
        },
        time: Time {
            hour: 0,
            minute: 0,
            second: 0,
            nanosecond: 0,
        },
    };

    let value = extract::<f64>(&DbValue::Timestamp(timestamp));
    assert!((value - 0.0).abs() < f64::EPSILON);
}

#[test]
fn journal_entry_equality_ignores_non_identity_fields() {
    // Verify equality ignores priority but respects message changes
    let base = sample_entry("4", "message");
    let mut other = base.clone();
    other.priority = "7".to_string();

    assert_eq!(base, other);

    let mut different = base.clone();
    different.message = "different".to_string();

    assert_ne!(base, different);
}

#[test]
fn api_error_type_maps_postgres_errors() {
    // Verify Postgres errors map to API error variants.
    let error = Error::ConnectionFailure("oops".to_string());
    let mapped = APIErrorType::Insert.of_postgres(error);

    match mapped {
        APIError::InsertError(message) => {
            assert!(message.contains("Connection failure: oops"));
        }
        _ => panic!("expected insert error"),
    }
}

#[test]
fn service_errors_strip_entries() {
    // Verify entries are stripped when converting summaries
    let errors = ServiceErrors {
        hostname: "host".to_string(),
        service_name: "service".to_string(),
        error_count: 3,
        min_priority: 2,
        started_at: 1.0,
        last_at: 2.0,
        entries: vec![1, 2, 3],
    };

    let no_entries = ServiceErrorsNoEntries::from(errors);
    assert_eq!(no_entries.hostname, "host");
    assert_eq!(no_entries.service_name, "service");
    assert_eq!(no_entries.error_count, 3);
    assert_eq!(no_entries.started_at, 1.0);
    assert_eq!(no_entries.last_at, 2.0);
}

#[test]
fn spike_event_severity_display() {
    // Verify severity display strings are stable
    assert_eq!(SpikeEventSeverity::Low.to_string(), "Low");
    assert_eq!(SpikeEventSeverity::Critical.to_string(), "Critical");
}
