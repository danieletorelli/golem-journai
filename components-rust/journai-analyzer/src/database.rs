use common_lib::database::*;
use common_lib::model::{APIError, APIErrorType, JournalEntry, JournalEntryId};
use golem_rust::bindings::golem::rdbms::postgres::*;

pub trait Database {
    fn get_entries(ids: Vec<JournalEntryId>) -> Result<Vec<JournalEntry>, APIError>;

    fn insert_analysis(
        hostname: String,
        analysis_type: String,
        model: String,
        summary: String,
        entry_ids: Vec<JournalEntryId>,
    ) -> Result<u64, APIError>;
}

impl Database for PostgresDatabase {
    fn get_entries(ids: Vec<JournalEntryId>) -> Result<Vec<JournalEntry>, APIError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let conn =
            PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

        let placeholders: String = (1..=ids.len())
            .map(|i| format!("${}", i))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("{} WHERE id IN ({})", BASE_FETCH_QUERY, placeholders);
        let params: Vec<DbValue> = ids.iter().map(|id| DbValue::Int8(*id as i64)).collect();

        log::debug!("Query: {}", sql);
        log::debug!("Params: {:?}", params);

        conn.query(&sql, params)
            .map(|result| result.rows.iter().map(|r| r.into()).collect())
            .map_err(|e| APIErrorType::Fetch.of_postgres(e))
    }

    fn insert_analysis(
        hostname: String,
        analysis_type: String,
        model: String,
        summary: String,
        entry_ids: Vec<JournalEntryId>,
    ) -> Result<u64, APIError> {
        let conn =
            PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

        // Insert into analyses table
        let insert_analysis_sql = r#"INSERT INTO analyses (hostname, analysis_type, model, summary) VALUES ($1, $2, $3, $4) RETURNING id"#;
        let params = vec![
            DbValue::Text(hostname),
            DbValue::Text(analysis_type),
            DbValue::Text(model),
            DbValue::Text(summary),
        ];

        log::debug!("Query: {}", insert_analysis_sql);
        log::debug!("Params: {:?}", params);

        let transaction = conn
            .begin_transaction()
            .map_err(|e| APIErrorType::Insert.of_postgres(e))?;

        let rollback_and_error = |e: APIError| {
            let _ = transaction.rollback();
            e
        };

        let result = conn
            .query(insert_analysis_sql, params)
            .map_err(|e| rollback_and_error(APIErrorType::Fetch.of_postgres(e)))?;

        let analysis_id: u64 = result
            .rows
            .first()
            .and_then(|row| row.values.first())
            .map(extract_int_unsigned)
            .ok_or_else(|| {
                rollback_and_error(
                    APIErrorType::Insert.of_string("Failed to retrieve analysis ID".to_string()),
                )
            })?;

        // Insert into the analyzed_entries table
        let insert_link_sql =
            r#"INSERT INTO analyzed_entries (entry_id, analysis_id) VALUES ($1, $2)"#;

        log::debug!("Query: {}", insert_link_sql);
        log::debug!("Inserting {} entry IDs", entry_ids.len());

        for entry_id in entry_ids {
            let link_params = vec![
                DbValue::Int8(entry_id as i64),
                DbValue::Int8(analysis_id as i64),
            ];

            conn.execute(insert_link_sql, link_params)
                .map_err(|e| rollback_and_error(APIErrorType::Insert.of_postgres(e)))?;
        }

        transaction
            .commit()
            .map_err(|e| APIErrorType::Insert.of_postgres(e))?;

        Ok(analysis_id)
    }
}

const BASE_FETCH_QUERY: &str = r#"SELECT boot_id, hostname, machine_id, priority, message, date, runtime_scope,
    pid, uid, gid, transport, syslog_facility, syslog_identifier,
    comm, exe, cmdline, unit, systemd_unit, systemd_slice, systemd_cgroup,
    code_line, code_file, job_id, job_result, job_type, invocation_id,
    source_monotonic_timestamp, source_boottime_timestamp FROM entries"#;
