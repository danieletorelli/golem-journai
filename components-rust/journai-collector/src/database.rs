use crate::model::{CollectorError, JournalEntry};
use golem_rust::bindings::golem::rdbms::postgres::*;
use std::env;
pub trait Database {
    fn insert_entries(entries: Vec<JournalEntry>) -> Result<u64, CollectorError>;

    fn get_entries(
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> Result<Vec<JournalEntry>, CollectorError>;
}

pub struct PostgresDatabase;

impl Database for PostgresDatabase {
    fn insert_entries(entries: Vec<JournalEntry>) -> Result<u64, CollectorError> {
        match PostgresDatabase::open_connection() {
            Ok(conn) => {
                let mut total_rows: u64 = 0;

                if let Ok(transaction) = conn.begin_transaction() {
                    for entry in entries {
                        conn.execute(
                            INSERT_QUERY,
                            vec![
                                DbValue::Text(entry.boot_id),
                                DbValue::Text(entry.hostname),
                                DbValue::Text(entry.machine_id),
                                DbValue::Text(entry.priority),
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
                            ],
                        )
                        .map(|rows| {
                            total_rows += rows;
                        })
                        .or_else(|e| {
                            transaction.rollback().map_err(error_to_insert_error)?;
                            Err(error_to_insert_error(e))
                        })?;
                    }

                    transaction.commit().map_err(error_to_insert_error)?;
                }

                Ok(total_rows)
            }
            Err(e) => Err(error_to_insert_error(e)),
        }
    }

    fn get_entries(
        since: Option<f64>,
        priority: Option<u8>,
        message_contains: Option<String>,
    ) -> Result<Vec<JournalEntry>, CollectorError> {
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<DbValue> = Vec::new();
        if let Some(since) = since {
            conditions.push(format!("date >= ${}", conditions.len() + 1));
            params.push(DbValue::Float8(since));
        }
        if let Some(priority) = priority {
            conditions.push(format!(
                "CAST(NULLIF(priority, '') AS INTEGER) <= ${}",
                conditions.len() + 1
            ));
            params.push(DbValue::Int2(priority as i16));
        }
        if let Some(message_contains) = message_contains {
            conditions.push(format!("message ILIKE ${}", conditions.len() + 1));
            params.push(DbValue::Text(format!("%{}%", message_contains)));
        }

        let sql = if conditions.is_empty() {
            BASE_FETCH_QUERY.to_string()
        } else {
            format!("{} WHERE {}", BASE_FETCH_QUERY, conditions.join(" AND "))
        };

        log::debug!("Query: {}", sql);
        log::debug!("Params: {:?}", params);

        PostgresDatabase::open_connection()
            .and_then(|conn| PostgresDatabase::create_table(&conn).map(|_| conn))
            .and_then(|conn| conn.query(&sql, params))
            .map(|result| result.rows.iter().map(row_to_entry).collect())
            .map_err(error_to_fetch_error)
    }
}

impl PostgresDatabase {
    fn open_connection() -> Result<DbConnection, Error> {
        if (env::var("DATABASE_TYPE").unwrap_or_else(|_| "none".to_string())) != "postgresql" {
            return Err(Error::ConnectionFailure(
                "PostgreSQL was not selected as database type".to_string(),
            ));
        }

        let user = env::var("DATABASE_USER").unwrap_or_else(|_| "journai".to_string());
        let password = env::var("DATABASE_PASSWORD").unwrap_or_else(|_| "".to_string());
        let host = env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".to_string());
        let port = env::var("DATABASE_PORT").unwrap_or_else(|_| "5432".to_string());
        let db = env::var("DATABASE_DB").unwrap_or_else(|_| "journai".to_string());

        let url = if password.is_empty() {
            format!("postgres://{}@{}:{}/{}", user, host, port, db)
        } else {
            format!("postgres://{}:{}@{}:{}/{}", user, password, host, port, db)
        };

        let masked_url = if password.is_empty() {
            format!("postgres://{}@{}:{}/{}", user, host, port, db)
        } else {
            format!("postgres://{}:***@{}:{}/{}", user, host, port, db)
        };

        log::info!("Connecting to {}", masked_url);

        match DbConnection::open(&url) {
            Ok(conn) => {
                if let Err(e) = conn.query("SELECT 1", vec![]) {
                    log::error!("Connection test failed: {:?}", e);
                    return Err(e);
                }
                Ok(conn)
            }
            Err(e) => {
                log::error!("Failed to open database connection: {:?}", e);
                Err(e)
            }
        }
    }

    fn create_table(connection: &DbConnection) -> Result<(), Error> {
        if let Err(e) = connection.execute(CREATE_TABLE_QUERY, vec![]) {
            log::error!("Failed to create table: {:?}", e);
            return Err(e);
        }
        Ok(())
    }
}

const CREATE_TABLE_QUERY: &str = r#"CREATE TABLE IF NOT EXISTS entries (
    id SERIAL PRIMARY KEY,
    boot_id TEXT NOT NULL,
    hostname TEXT NOT NULL,
    machine_id TEXT NOT NULL,
    priority TEXT NOT NULL,
    message TEXT NOT NULL,
    date DOUBLE PRECISION NOT NULL,
    runtime_scope TEXT NOT NULL,
    pid TEXT,
    uid TEXT,
    gid TEXT,
    transport TEXT,
    syslog_facility TEXT,
    syslog_identifier TEXT,
    comm TEXT,
    exe TEXT,
    cmdline TEXT,
    unit TEXT,
    systemd_unit TEXT,
    systemd_slice TEXT,
    systemd_cgroup TEXT,
    code_line TEXT,
    code_file TEXT,
    job_id TEXT,
    job_result TEXT,
    job_type TEXT,
    invocation_id TEXT,
    source_monotonic_timestamp TEXT,
    source_boottime_timestamp TEXT);"#;

const INSERT_QUERY: &str = r#"INSERT INTO entries (
            boot_id, hostname, machine_id, priority, message, date, runtime_scope,
            pid, uid, gid, transport, syslog_facility, syslog_identifier,
            comm, exe, cmdline, unit, systemd_unit, systemd_slice, systemd_cgroup,
            code_line, code_file, job_id, job_result, job_type, invocation_id,
            source_monotonic_timestamp, source_boottime_timestamp
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28)"#;

const BASE_FETCH_QUERY: &str = r#"SELECT boot_id, hostname, machine_id, priority, message, date, runtime_scope,
    pid, uid, gid, transport, syslog_facility, syslog_identifier,
    comm, exe, cmdline, unit, systemd_unit, systemd_slice, systemd_cgroup,
    code_line, code_file, job_id, job_result, job_type, invocation_id,
    source_monotonic_timestamp, source_boottime_timestamp FROM entries"#;

fn row_to_entry(row: &DbRow) -> JournalEntry {
    let values = &row.values;
    JournalEntry {
        boot_id: extract_text(&values[0]),
        hostname: extract_text(&values[1]),
        machine_id: extract_text(&values[2]),
        priority: extract_text(&values[3]),
        message: extract_text(&values[4]),
        date: extract_float8(&values[5]),
        runtime_scope: extract_text(&values[6]),
        pid: extract_optional_text(&values[7]),
        uid: extract_optional_text(&values[8]),
        gid: extract_optional_text(&values[9]),
        transport: extract_optional_text(&values[10]),
        syslog_facility: extract_optional_text(&values[11]),
        syslog_identifier: extract_optional_text(&values[12]),
        comm: extract_optional_text(&values[13]),
        exe: extract_optional_text(&values[14]),
        cmdline: extract_optional_text(&values[15]),
        unit: extract_optional_text(&values[16]),
        systemd_unit: extract_optional_text(&values[17]),
        systemd_slice: extract_optional_text(&values[18]),
        systemd_cgroup: extract_optional_text(&values[19]),
        code_line: extract_optional_text(&values[20]),
        code_file: extract_optional_text(&values[21]),
        job_id: extract_optional_text(&values[22]),
        job_result: extract_optional_text(&values[23]),
        job_type: extract_optional_text(&values[24]),
        invocation_id: extract_optional_text(&values[25]),
        source_monotonic_timestamp: extract_optional_text(&values[26]),
        source_boottime_timestamp: extract_optional_text(&values[27]),
    }
}

fn extract_text(value: &DbValue) -> String {
    match value {
        DbValue::Text(s) => s.clone(),
        _ => String::new(),
    }
}

fn extract_float8(value: &DbValue) -> f64 {
    match value {
        DbValue::Float8(f) => *f,
        _ => 0.0,
    }
}

fn extract_optional_text(value: &DbValue) -> Option<String> {
    match value {
        DbValue::Text(s) => Some(s.clone()),
        DbValue::Null => None,
        _ => None,
    }
}

fn error_to_insert_error(e: Error) -> CollectorError {
    match e {
        Error::ConnectionFailure(e) => {
            CollectorError::InsertError(format!("Connection failure: {}", e))
        }
        Error::QueryExecutionFailure(e) => {
            CollectorError::InsertError(format!("Query execution failure: {}", e))
        }
        Error::QueryParameterFailure(e) => {
            CollectorError::InsertError(format!("Query parameter failure: {}", e))
        }
        Error::QueryResponseFailure(e) => {
            CollectorError::InsertError(format!("Query response failure: {}", e))
        }
        Error::Other(e) => CollectorError::InsertError(e),
    }
}

fn error_to_fetch_error(e: Error) -> CollectorError {
    match e {
        Error::ConnectionFailure(e) => {
            CollectorError::FetchError(format!("Connection failure: {}", e))
        }
        Error::QueryExecutionFailure(e) => {
            CollectorError::FetchError(format!("Query execution failure: {}", e))
        }
        Error::QueryParameterFailure(e) => {
            CollectorError::FetchError(format!("Query parameter failure: {}", e))
        }
        Error::QueryResponseFailure(e) => {
            CollectorError::FetchError(format!("Query response failure: {}", e))
        }
        Error::Other(e) => CollectorError::FetchError(e),
    }
}
