use common_lib::database::*;
use common_lib::model::{
    APIError, APIErrorType, JournalEntry, ServiceErrors, ServiceErrorsBuilder,
};
use golem_rust::bindings::golem::rdbms::postgres::*;

pub fn insert_entries(entries: Vec<JournalEntry>) -> Result<u64, APIError> {
    if entries.is_empty() {
        return Ok(0);
    }

    const FIELD_COUNT: usize = 28;
    const CHUNK_SIZE: usize = MAX_PARAMS / FIELD_COUNT;

    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Insert.of_postgres(e))?;
    let mut total_inserted = 0;

    for chunk in entries.chunks(CHUNK_SIZE) {
        let entries_count = chunk.len();
        let mut params = Vec::with_capacity(entries_count * FIELD_COUNT);

        let placeholders: String = (0..entries_count)
            .map(|i| {
                let base = i * FIELD_COUNT;
                let group = (1..=FIELD_COUNT)
                    .map(|j| format!("${}", base + j))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({})", group)
            })
            .collect::<Vec<_>>()
            .join(", ");

        for entry in chunk {
            params.push(DbValue::Text(entry.boot_id.clone()));
            params.push(DbValue::Text(entry.hostname.clone()));
            params.push(DbValue::Text(entry.machine_id.clone()));
            params.push(DbValue::Int2(
                entry.priority.parse::<u8>().unwrap_or(0) as i16
            ));
            params.push(DbValue::Text(entry.message.clone()));
            params.push(DbValue::Float8(entry.date));
            params.push(DbValue::Text(entry.runtime_scope.clone()));

            let mut push_optional = |opt: &Option<String>| {
                params.push(
                    opt.as_ref()
                        .map_or(DbValue::Null, |v| DbValue::Text(v.clone())),
                );
            };

            push_optional(&entry.pid);
            push_optional(&entry.uid);
            push_optional(&entry.gid);
            push_optional(&entry.transport);
            push_optional(&entry.syslog_facility);
            push_optional(&entry.syslog_identifier);
            push_optional(&entry.comm);
            push_optional(&entry.exe);
            push_optional(&entry.cmdline);
            push_optional(&entry.unit);
            push_optional(&entry.systemd_unit);
            push_optional(&entry.systemd_slice);
            push_optional(&entry.systemd_cgroup);
            push_optional(&entry.code_line);
            push_optional(&entry.code_file);
            push_optional(&entry.job_id);
            push_optional(&entry.job_result);
            push_optional(&entry.job_type);
            push_optional(&entry.invocation_id);
            push_optional(&entry.source_monotonic_timestamp);
            push_optional(&entry.source_boottime_timestamp);
        }

        let sql = format!(
            "{} VALUES {} ON CONFLICT DO NOTHING",
            BASE_INSERT_QUERY, placeholders
        );

        if should_log_queries() {
            log::debug!("Query: {}", sql);
            log::trace!("Params: {:?}", params);
        }

        let inserted = conn
            .execute(&sql, params)
            .map_err(|e| APIErrorType::Insert.of_postgres(e))?;
        total_inserted += inserted;
    }

    Ok(total_inserted)
}

pub fn get_entries(
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

    if should_log_queries() {
        log::debug!("Query: {}", sql);
        log::trace!("Params: {:?}", params);
    }

    conn.query(&sql, params)
        .map(|result| result.rows.iter().map(|r| r.into()).collect())
        .map_err(|e| APIErrorType::Fetch.of_postgres(e))
}

pub fn get_last_analysis_timestamp(hostname: String) -> Result<Option<f64>, APIError> {
    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;
    let params: Vec<DbValue> = vec![DbValue::Text(hostname)];

    if should_log_queries() {
        log::debug!("Query: {}", FETCH_LAST_ANALYSIS_TIMESTAMP_QUERY);
        log::trace!("Params: {:?}", params);
    }

    conn.query(FETCH_LAST_ANALYSIS_TIMESTAMP_QUERY, params)
        .map(|result| result.rows.first().map(|row| extract(&row.values[0])))
        .map_err(|e| APIErrorType::Fetch.of_postgres(e))
}

pub fn get_error_spikes(hostname: String, since: f64) -> Result<Vec<ServiceErrors>, APIError> {
    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;
    let params: Vec<DbValue> = vec![DbValue::Text(hostname.to_string()), DbValue::Float8(since)];

    if should_log_queries() {
        log::debug!("Query: {}", FETCH_ERROR_SPIKES_QUERY);
        log::trace!("Params: {:?}", params);
    }

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
    COALESCE(unit, syslog_identifier, comm) AS service_name,
    COUNT(*) AS error_count,
    MIN(priority) AS min_priority,
    MIN(date) AS first_error,
    MAX(date) AS last_error,
    ARRAY_AGG(id ORDER BY date DESC) AS entry_ids
FROM entries
WHERE
    hostname = $1
  AND date >= $2
  AND priority <= 3
  AND COALESCE(unit, syslog_identifier, comm) IS NOT NULL
GROUP BY service_name
HAVING COUNT(*) > 5
ORDER BY error_count DESC;"#;

const FETCH_LAST_ANALYSIS_TIMESTAMP_QUERY: &str =
    "SELECT DATE_PART('epoch', MAX(analysed_at)) FROM analyses WHERE hostname = $1";
