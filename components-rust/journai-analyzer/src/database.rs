use common_lib::database::*;
use common_lib::model::{APIError, APIErrorType, JournalEntry, JournalEntryId};
use golem_rust::bindings::golem::rdbms::postgres::*;

pub fn get_entries_by_ids(
    ids: Vec<JournalEntryId>,
    limit: u16,
) -> Result<Vec<JournalEntry>, APIError> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

    let mut all_entries = Vec::new();

    for chunk in ids.chunks(MAX_PARAMS) {
        let placeholders: String = (1..=chunk.len())
            .map(|i| format!("${}", i))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "{} WHERE id IN ({}) LIMIT {}",
            BASE_FETCH_QUERY,
            placeholders,
            limit as usize - all_entries.len()
        );
        let params: Vec<DbValue> = chunk.iter().map(|id| DbValue::Int8(*id as i64)).collect();

        if should_log_queries() {
            log::debug!("Query: {}", sql);
            log::trace!("Params: {:?}", params);
        }

        let result = conn
            .query(&sql, params)
            .map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

        let chunk_entries: Vec<JournalEntry> = result.rows.iter().map(|r| r.into()).collect();
        all_entries.extend(chunk_entries);

        if all_entries.len() >= limit as usize {
            break;
        }
    }

    Ok(all_entries)
}

pub fn insert_analysis(
    hostname: String,
    analysis_type: String,
    model: String,
    summary: String,
    entry_ids: Vec<JournalEntryId>,
) -> Result<u64, APIError> {
    if entry_ids.is_empty() {
        return Ok(0);
    }

    const FIELD_COUNT: usize = 2;
    const CHUNK_SIZE: usize = MAX_PARAMS / FIELD_COUNT;

    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

    let insert_analysis_sql = r#"INSERT INTO analyses (hostname, analysis_type, model, summary) VALUES ($1, $2, $3, $4) RETURNING id"#;
    let insert_analysis_params = vec![
        DbValue::Text(hostname),
        DbValue::Text(analysis_type),
        DbValue::Text(model),
        DbValue::Text(summary),
    ];

    if should_log_queries() {
        log::debug!("Query: {}", insert_analysis_sql);
        log::trace!("Params: {:?}", insert_analysis_params);
    }

    let transaction = conn
        .begin_transaction()
        .map_err(|e| APIErrorType::Insert.of_postgres(e))?;

    let rollback_and_error = |e: APIError| {
        let _ = transaction.rollback();
        e
    };

    let result = conn
        .query(insert_analysis_sql, insert_analysis_params)
        .map_err(|e| rollback_and_error(APIErrorType::Fetch.of_postgres(e)))?;

    let analysis_id: u64 = result
        .rows
        .first()
        .and_then(|row| row.values.first())
        .map(extract)
        .ok_or_else(|| {
            rollback_and_error(
                APIErrorType::Insert.of_string("Failed to retrieve analysis ID".to_string()),
            )
        })?;

    for chunk in entry_ids.chunks(CHUNK_SIZE) {
        let mut insert_link_params = Vec::with_capacity(chunk.len() * FIELD_COUNT);
        let placeholders: String = chunk
            .iter()
            .enumerate()
            .map(|(i, entry_id)| {
                let base = i * 2;
                insert_link_params.push(DbValue::Int8(*entry_id as i64));
                insert_link_params.push(DbValue::Int8(analysis_id as i64));
                format!("(${}, ${})", base + 1, base + 2)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let insert_link_sql = format!(
            "INSERT INTO analyzed_entries (entry_id, analysis_id) VALUES {}",
            placeholders
        );

        if should_log_queries() {
            log::debug!("Query: {}", insert_link_sql);
            log::trace!("Params: {:?}", insert_link_params);
        }

        conn.execute(&insert_link_sql, insert_link_params)
            .map_err(|e| rollback_and_error(APIErrorType::Insert.of_postgres(e)))?;
    }

    transaction
        .commit()
        .map_err(|e| APIErrorType::Insert.of_postgres(e))?;

    Ok(analysis_id)
}

const BASE_FETCH_QUERY: &str = r#"SELECT boot_id, hostname, machine_id, priority, message, date, runtime_scope,
    pid, uid, gid, transport, syslog_facility, syslog_identifier,
    comm, exe, cmdline, unit, systemd_unit, systemd_slice, systemd_cgroup,
    code_line, code_file, job_id, job_result, job_type, invocation_id,
    source_monotonic_timestamp, source_boottime_timestamp FROM entries"#;
