use common_lib::database::*;
use common_lib::model::{
    APIError, APIErrorType, JournalEntry, ServiceErrors, ServiceErrorsBuilder,
};
use golem_rust::bindings::golem::rdbms::postgres::*;

pub trait Database {
    fn insert_entries(entries: Vec<JournalEntry>) -> Result<u64, APIError>;

    fn get_entries(
        hostname: String,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> Result<Vec<JournalEntry>, APIError>;

    fn get_last_analysis_timestamp(hostname: String) -> Result<Option<f64>, APIError>;

    fn get_error_spikes(hostname: String, since: f64) -> Result<Vec<ServiceErrors>, APIError>;
}

impl Database for PostgresDatabase {
    fn insert_entries(entries: Vec<JournalEntry>) -> Result<u64, APIError> {
        if entries.is_empty() {
            return Ok(0);
        }

        const FIELD_COUNT: usize = 28;

        let placeholders: Vec<String> = entries
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let base = i * FIELD_COUNT;
                let params: Vec<String> = (1..=FIELD_COUNT)
                    .map(|i| format!("${}", base + i))
                    .collect();
                format!("({})", params.join(", "))
            })
            .collect();

        let sql = format!(
            "{} VALUES {} ON CONFLICT DO NOTHING",
            BASE_INSERT_QUERY,
            placeholders.join(", ")
        );

        let mut params: Vec<DbValue> = Vec::new();
        for entry in entries {
            params.extend(vec![
                DbValue::Text(entry.boot_id),
                DbValue::Text(entry.hostname),
                DbValue::Text(entry.machine_id),
                DbValue::Int2(entry.priority.parse::<u8>().unwrap_or(0) as i16),
                DbValue::Text(entry.message),
                DbValue::Float8(entry.date),
                DbValue::Text(entry.runtime_scope),
                entry.pid.map_or(DbValue::Null, DbValue::Text),
                entry.uid.map_or(DbValue::Null, DbValue::Text),
                entry.gid.map_or(DbValue::Null, DbValue::Text),
                entry.transport.map_or(DbValue::Null, DbValue::Text),
                entry.syslog_facility.map_or(DbValue::Null, DbValue::Text),
                entry.syslog_identifier.map_or(DbValue::Null, DbValue::Text),
                entry.comm.map_or(DbValue::Null, DbValue::Text),
                entry.exe.map_or(DbValue::Null, DbValue::Text),
                entry.cmdline.map_or(DbValue::Null, DbValue::Text),
                entry.unit.map_or(DbValue::Null, DbValue::Text),
                entry.systemd_unit.map_or(DbValue::Null, DbValue::Text),
                entry.systemd_slice.map_or(DbValue::Null, DbValue::Text),
                entry.systemd_cgroup.map_or(DbValue::Null, DbValue::Text),
                entry.code_line.map_or(DbValue::Null, DbValue::Text),
                entry.code_file.map_or(DbValue::Null, DbValue::Text),
                entry.job_id.map_or(DbValue::Null, DbValue::Text),
                entry.job_result.map_or(DbValue::Null, DbValue::Text),
                entry.job_type.map_or(DbValue::Null, DbValue::Text),
                entry.invocation_id.map_or(DbValue::Null, DbValue::Text),
                entry
                    .source_monotonic_timestamp
                    .map_or(DbValue::Null, DbValue::Text),
                entry
                    .source_boottime_timestamp
                    .map_or(DbValue::Null, DbValue::Text),
            ]);
        }

        log::debug!("Query: {}", sql);
        log::debug!("Params: {:?}", params);

        let conn =
            PostgresDatabase::open_connection().map_err(|e| APIErrorType::Insert.of_postgres(e))?;
        PostgresDatabase::create_table(&conn).map_err(|e| APIErrorType::Insert.of_postgres(e))?;
        conn.execute(&sql, params)
            .map_err(|e| APIErrorType::Insert.of_postgres(e))
    }

    fn get_entries(
        hostname: String,
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> Result<Vec<JournalEntry>, APIError> {
        let conn =
            PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<DbValue> = vec![DbValue::Text(hostname)];
        if let Some(since) = since {
            conditions.push(format!("date >= ${}", params.len() + 1));
            params.push(DbValue::Float8(since));
        }
        if let Some(priority) = priority {
            conditions.push(format!("priority <= ${}", params.len() + 1));
            params.push(DbValue::Int2(priority as i16));
        }
        if let Some(message_contains) = message_contains {
            conditions.push(format!("message ILIKE ${}", params.len() + 1));
            params.push(DbValue::Text(format!("%{}%", message_contains)));
        }

        let sql = if conditions.is_empty() {
            BASE_FETCH_QUERY.to_string()
        } else {
            format!("{} AND {}", BASE_FETCH_QUERY, conditions.join(" AND "))
        };

        log::debug!("Query: {}", sql);
        log::debug!("Params: {:?}", params);

        PostgresDatabase::create_table(&conn).map_err(|e| APIErrorType::Insert.of_postgres(e))?;
        conn.query(&sql, params)
            .map(|result| result.rows.iter().map(|r| r.into()).collect())
            .map_err(|e| APIErrorType::Fetch.of_postgres(e))
    }

    fn get_last_analysis_timestamp(hostname: String) -> Result<Option<f64>, APIError> {
        let conn =
            PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;
        let params: Vec<DbValue> = vec![DbValue::Text(hostname)];

        log::debug!("Query: {}", FETCH_LAST_ANALYSIS_TIMESTAMP_QUERY);
        log::debug!("Params: {:?}", params);

        conn.query(FETCH_LAST_ANALYSIS_TIMESTAMP_QUERY, params)
            .map(|result| result.rows.first().map(|row| extract_float(&row.values[0])))
            .map_err(|e| APIErrorType::Fetch.of_postgres(e))
    }

    fn get_error_spikes(hostname: String, since: f64) -> Result<Vec<ServiceErrors>, APIError> {
        let conn =
            PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;
        let params: Vec<DbValue> =
            vec![DbValue::Text(hostname.to_string()), DbValue::Float8(since)];

        log::debug!("Query: {}", FETCH_ERROR_SPIKES_QUERY);
        log::debug!("Params: {:?}", params);

        conn.query(FETCH_ERROR_SPIKES_QUERY, params)
            .map(|result| {
                result
                    .rows
                    .iter()
                    .map(|row| {
                        ServiceErrors::from(ServiceErrorsBuilder {
                            hostname: hostname.to_string(),
                            row,
                        })
                    })
                    .collect()
            })
            .map_err(|e| APIErrorType::Fetch.of_postgres(e))
    }
}

const BASE_INSERT_QUERY: &str = r#"INSERT INTO entries (
            boot_id, hostname, machine_id, priority, message, date, runtime_scope,
            pid, uid, gid, transport, syslog_facility, syslog_identifier,
            comm, exe, cmdline, unit, systemd_unit, systemd_slice, systemd_cgroup,
            code_line, code_file, job_id, job_result, job_type, invocation_id,
            source_monotonic_timestamp, source_boottime_timestamp
        )"#;

const BASE_FETCH_QUERY: &str = r#"SELECT boot_id, hostname, machine_id, priority, message, date, runtime_scope,
    pid, uid, gid, transport, syslog_facility, syslog_identifier,
    comm, exe, cmdline, unit, systemd_unit, systemd_slice, systemd_cgroup,
    code_line, code_file, job_id, job_result, job_type, invocation_id,
    source_monotonic_timestamp, source_boottime_timestamp FROM entries WHERE hostname = $1"#;

const FETCH_ERROR_SPIKES_QUERY: &str = r#"SELECT
    COALESCE(unit, syslog_identifier, comm, 'unknown') AS service_name,
    COUNT(*) AS error_count,
    MIN(priority) AS min_priority,
    MIN(date) AS first_error,
    MAX(date) AS last_error,
    ARRAY_AGG(id ORDER BY date DESC) AS entry_ids
FROM entries
WHERE
    hostname = $1 AND date >= $2
  AND priority <= 3
GROUP BY service_name
HAVING COUNT(*) > 5
ORDER BY error_count DESC;"#;

const FETCH_LAST_ANALYSIS_TIMESTAMP_QUERY: &str =
    "SELECT MAX(analysed_at) FROM analyses WHERE hostname = $1";
