use golem_rust::bindings::golem::rdbms::postgres::{DbConnection, DbValue, Error};
use std::env;

pub struct PostgresDatabase;

impl PostgresDatabase {
    pub fn open_connection() -> Result<DbConnection, Error> {
        if env::var("DATABASE_TYPE").unwrap_or_else(|_| "none".to_string()) != "postgresql" {
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

        log::debug!("Connecting to {}", masked_url);

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

    pub fn create_table(connection: &DbConnection) -> Result<(), Error> {
        if env::var("DATABASE_INIT").unwrap_or_else(|_| "false".to_string()) != "true" {
            return Ok(());
        }

        log::debug!("Initializing database");

        let transaction = connection.begin_transaction()?;

        let all_queries = CREATE_TABLES_QUERY
            .iter()
            .chain(CREATE_INDEXES_QUERIES.iter());

        for query in all_queries {
            if let Err(e) = connection.execute(query, vec![]) {
                log::error!("Failed to execute query: {:?}", e);
                let _ = transaction.rollback();
                return Err(e);
            }
        }

        transaction.commit()?;
        log::debug!("Database initialized successfully");
        Ok(())
    }
}

const CREATE_TABLES_QUERY: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS entries (
        id SERIAL PRIMARY KEY,
        boot_id TEXT NOT NULL,
        hostname TEXT NOT NULL,
        machine_id TEXT NOT NULL,
        priority SMALLINT NOT NULL CHECK (priority BETWEEN 0 AND 7),
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
        source_boottime_timestamp TEXT);"#,
    r#"CREATE TABLE IF NOT EXISTS analyses (
        id SERIAL PRIMARY KEY,
        hostname TEXT NOT NULL,
        analysis_type TEXT NOT NULL CHECK (analysis_type IN ('spike', 'report')),
        model TEXT NOT NULL,
        summary TEXT NOT NULL,
        analysed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);"#,
    r#"CREATE TABLE IF NOT EXISTS analyzed_entries (
        entry_id INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
        analysis_id INTEGER NOT NULL REFERENCES analyses(id) ON DELETE CASCADE,
        PRIMARY KEY (entry_id, analysis_id));"#,
];

const CREATE_INDEXES_QUERIES: &[&str] = &[
    r#"CREATE EXTENSION IF NOT EXISTS pg_trgm;"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_entries_unique
       ON entries(boot_id, hostname, machine_id, md5(message), COALESCE(pid, ''), COALESCE(uid, ''), COALESCE(gid, ''), date);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_date ON entries(date);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_priority ON entries(priority);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_hostname ON entries(hostname);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_hostname_date ON entries(hostname, date DESC);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_message ON entries USING GIN(message gin_trgm_ops);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_hostname_priority ON entries(hostname, priority);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_hostname_date_priority ON entries(hostname, date DESC, priority);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_analyses_hostname ON analyses(hostname);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_analyses_hostname_analysed_at ON analyses(hostname, analysed_at DESC);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_analyzed_entries_analysis_id ON analyzed_entries(analysis_id);"#,
];

pub fn extract_text(value: &DbValue) -> String {
    match value {
        DbValue::Text(s) => s.clone(),
        _ => String::new(),
    }
}

pub fn extract_float(value: &DbValue) -> f64 {
    match value {
        DbValue::Float8(f) => *f,
        DbValue::Float4(f) => *f as f64,
        _ => 0.0,
    }
}

pub fn extract_int_unsigned(value: &DbValue) -> u64 {
    match value {
        DbValue::Int8(f) => *f as u64,
        DbValue::Int4(f) => *f as u64,
        DbValue::Int2(f) => *f as u64,
        _ => 0,
    }
}

pub fn extract_short_unsigned(value: &DbValue) -> u8 {
    match value {
        DbValue::Int8(f) => *f as u8,
        DbValue::Int4(f) => *f as u8,
        DbValue::Int2(f) => *f as u8,
        _ => 0,
    }
}

pub fn extract_optional_text(value: &DbValue) -> Option<String> {
    match value {
        DbValue::Text(s) => Some(s.clone()),
        DbValue::Null => None,
        _ => None,
    }
}
