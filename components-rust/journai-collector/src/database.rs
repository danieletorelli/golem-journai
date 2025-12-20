use crate::model::JournalEntry;
use golem_rust::bindings::golem::rdbms::postgres::*;
use std::env;

pub trait Database {
    fn new() -> Self;

    fn insert_entries(&self, entries: Vec<JournalEntry>) -> Result<u64, Error>;

    fn _get_entries(&self) -> Result<Vec<JournalEntry>, Error>;
}

pub struct PostgresDatabase {
    connection: Option<DbConnection>,
}

impl Database for PostgresDatabase {
    fn new() -> Self {
        if (env::var("DATABASE_TYPE").unwrap_or_else(|_| "none".to_string())) != "postgresql" {
            return Self { connection: None };
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

        let conn = DbConnection::open(&url).expect("Failed to connect to database");

        conn.query("SELECT 1", vec![])
            .expect("Failed to test database connection");

        conn.execute(
            r#"CREATE TABLE IF NOT EXISTS entries (
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
            source_boottime_timestamp TEXT
        );"#,
            vec![],
        )
        .expect("Failed to initialize the database");

        Self {
            connection: Some(conn),
        }
    }

    fn insert_entries(&self, entries: Vec<JournalEntry>) -> Result<u64, Error> {
        if let Some(conn) = self.connection.as_ref() {
            let mut total_rows = 0;
            let transaction = conn
                .begin_transaction()
                .expect("Failed to begin the transaction");

            for entry in entries {
                let sql = r#"INSERT INTO entries (
            boot_id, hostname, machine_id, priority, message, date, runtime_scope,
            pid, uid, gid, transport, syslog_facility, syslog_identifier,
            comm, exe, cmdline, unit, systemd_unit, systemd_slice, systemd_cgroup,
            code_line, code_file, job_id, job_result, job_type, invocation_id,
            source_monotonic_timestamp, source_boottime_timestamp
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28)"#;

                conn.execute(
                    sql,
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
                .map_err(|e| {
                    transaction
                        .rollback()
                        .expect("Failed to rollback the transaction");
                    e
                })
                .expect("Failed to insert the entries");
            }

            transaction
                .commit()
                .expect("Failed to commit the transaction");

            Ok(total_rows)
        } else {
            Ok(0)
        }
    }

    fn _get_entries(&self) -> Result<Vec<JournalEntry>, Error> {
        let sql = "SELECT boot_id, hostname, machine_id, priority, message, date, runtime_scope,
        pid, uid, gid, transport, syslog_facility, syslog_identifier,
        comm, exe, cmdline, unit, systemd_unit, systemd_slice, systemd_cgroup,
        code_line, code_file, job_id, job_result, job_type, invocation_id,
        source_monotonic_timestamp, source_boottime_timestamp FROM entries";

        if let Some(conn) = self.connection.as_ref() {
            let result = conn.query(sql, vec![])?;
            let mut entries = Vec::new();

            for row in result.rows {
                let entry = JournalEntry {
                    boot_id: extract_text(&row.values[0]),
                    hostname: extract_text(&row.values[1]),
                    machine_id: extract_text(&row.values[2]),
                    priority: extract_text(&row.values[3]),
                    message: extract_text(&row.values[4]),
                    date: extract_float8(&row.values[5]),
                    runtime_scope: extract_text(&row.values[6]),
                    pid: extract_optional_text(&row.values[7]),
                    uid: extract_optional_text(&row.values[8]),
                    gid: extract_optional_text(&row.values[9]),
                    transport: extract_optional_text(&row.values[10]),
                    syslog_facility: extract_optional_text(&row.values[11]),
                    syslog_identifier: extract_optional_text(&row.values[12]),
                    comm: extract_optional_text(&row.values[13]),
                    exe: extract_optional_text(&row.values[14]),
                    cmdline: extract_optional_text(&row.values[15]),
                    unit: extract_optional_text(&row.values[16]),
                    systemd_unit: extract_optional_text(&row.values[17]),
                    systemd_slice: extract_optional_text(&row.values[18]),
                    systemd_cgroup: extract_optional_text(&row.values[19]),
                    code_line: extract_optional_text(&row.values[20]),
                    code_file: extract_optional_text(&row.values[21]),
                    job_id: extract_optional_text(&row.values[22]),
                    job_result: extract_optional_text(&row.values[23]),
                    job_type: extract_optional_text(&row.values[24]),
                    invocation_id: extract_optional_text(&row.values[25]),
                    source_monotonic_timestamp: extract_optional_text(&row.values[26]),
                    source_boottime_timestamp: extract_optional_text(&row.values[27]),
                };

                entries.push(entry);
            }

            Ok(entries)
        } else {
            Ok(vec![])
        }
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
