use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use golem_rust::bindings::golem::rdbms::postgres::{DbConnection, DbValue, Error};
use std::env;

pub struct PostgresDatabase;

pub const MAX_PARAMS: usize = 65535;

pub fn should_log_queries() -> bool {
    env::var("DATABASE_QUERY_LOG").is_ok_and(|v| v == "true")
}

impl PostgresDatabase {
    pub fn open_connection() -> Result<DbConnection, Error> {
        if env::var("DATABASE_TYPE").unwrap_or_default() != "postgresql" {
            return Err(Error::ConnectionFailure(
                "PostgreSQL was not selected as database type".to_string(),
            ));
        }

        let user = env::var("DATABASE_USER").unwrap_or_else(|_| "journai".to_string());
        let password = env::var("DATABASE_PASSWORD").ok();
        let host = env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".to_string());
        let port = env::var("DATABASE_PORT")
            .unwrap_or_else(|_| "5432".to_string())
            .parse::<u16>()
            .map_err(|_| Error::ConnectionFailure("Invalid DATABASE_PORT value".to_string()))?
            .to_string();
        let db = env::var("DATABASE_DB").unwrap_or_else(|_| "journai".to_string());

        let url = match &password {
            Some(p) => format!("postgres://{}:{}@{}:{}/{}", user, p, host, port, db),
            None => format!("postgres://{}@{}:{}/{}", user, host, port, db),
        };

        let masked_url = match &password {
            Some(_) => format!("postgres://{}:***@{}:{}/{}", user, host, port, db),
            None => format!("postgres://{}@{}:{}/{}", user, host, port, db),
        };

        log::debug!("Connecting to {}", masked_url);

        match DbConnection::open(&url) {
            Ok(conn) => {
                if let Err(e) = conn.query("SELECT 1", vec![]) {
                    log::error!("Connection test failed: {:?}", e);
                    return Err(e);
                }

                if env::var("DATABASE_INIT").is_ok_and(|v| v == "true") {
                    Self::create_table(&conn)?;
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
        log::debug!("Initializing database");

        let trgm_available = match connection.execute(CREATE_EXTENSIONS_QUERY, vec![]) {
            Ok(_) => true,
            Err(e) => {
                log::warn!(
                    "Skipping extension creation due to error: {:?}. Query: {}",
                    e,
                    CREATE_EXTENSIONS_QUERY
                );
                false
            }
        };

        let transaction = connection.begin_transaction()?;

        let all_queries = CREATE_TABLES_QUERY.iter().chain(CREATE_INDEXES_QUERIES);

        for query in all_queries {
            if !trgm_available && query.contains("gin_trgm_ops") {
                log::warn!("Skipping trigram index creation (pg_trgm not available).");
                continue;
            }
            if let Err(e) = transaction.execute(query, vec![]) {
                log::error!("Failed to execute query: {:?}. Query: {}", e, query);
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
        boot_id TEXT NOT NULL CHECK (btrim(boot_id) <> ''),
        hostname TEXT NOT NULL CHECK (btrim(hostname) <> ''),
        machine_id TEXT NOT NULL CHECK (btrim(machine_id) <> ''),
        priority SMALLINT NOT NULL CHECK (priority BETWEEN 0 AND 7),
        message TEXT NOT NULL CHECK (btrim(message) <> ''),
        date DOUBLE PRECISION NOT NULL,
        runtime_scope TEXT NOT NULL CHECK (btrim(runtime_scope) <> ''),
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
        hostname TEXT NOT NULL CHECK (btrim(hostname) <> ''),
        analysis_type TEXT NOT NULL CHECK (analysis_type IN ('spike', 'report')),
        model TEXT NOT NULL CHECK (btrim(model) <> ''),
        summary TEXT NOT NULL CHECK (btrim(summary) <> ''),
        severity TEXT NOT NULL CHECK (severity IN ('Low', 'Medium', 'High', 'Critical')),
        needs_user_action BOOLEAN NOT NULL,
        analysed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);"#,
    r#"CREATE TABLE IF NOT EXISTS analyzed_entries (
        entry_id INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
        analysis_id INTEGER NOT NULL REFERENCES analyses(id) ON DELETE CASCADE,
        PRIMARY KEY (entry_id, analysis_id));"#,
];

const CREATE_EXTENSIONS_QUERY: &str = r#"CREATE EXTENSION IF NOT EXISTS pg_trgm;"#;

const CREATE_INDEXES_QUERIES: &[&str] = &[
    r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_entries_unique
       ON entries(boot_id, hostname, machine_id, md5(message), COALESCE(pid, ''), COALESCE(uid, ''), COALESCE(gid, ''), date);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_hostname_date_priority ON entries(hostname, date DESC, priority);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_entries_message ON entries USING GIN(message gin_trgm_ops);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_analyses_hostname_analysed_at ON analyses(hostname, analysed_at DESC);"#,
    r#"CREATE INDEX IF NOT EXISTS idx_analyzed_entries_analysis_id ON analyzed_entries(analysis_id);"#,
];

pub trait FromDbValue: Sized {
    fn from_db_value(value: &DbValue) -> Self;
}

impl FromDbValue for String {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Text(s) => s.clone(),
            DbValue::Int2(i) => i.to_string(),
            DbValue::Int4(i) => i.to_string(),
            DbValue::Int8(i) => i.to_string(),
            DbValue::Float4(f) => f.to_string(),
            DbValue::Float8(f) => f.to_string(),
            DbValue::Timestamp(t) => {
                let ndt = NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(t.date.year, t.date.month as u32, t.date.day as u32)
                        .unwrap_or_default(),
                    NaiveTime::from_hms_nano_opt(
                        t.time.hour as u32,
                        t.time.minute as u32,
                        t.time.second as u32,
                        t.time.nanosecond,
                    )
                    .unwrap_or_default(),
                );
                ndt.format("%Y-%m-%d %H:%M:%S").to_string()
            }
            DbValue::Timestamptz(t) => {
                let ndt = NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(
                        t.timestamp.date.year,
                        t.timestamp.date.month as u32,
                        t.timestamp.date.day as u32,
                    )
                    .unwrap_or_default(),
                    NaiveTime::from_hms_nano_opt(
                        t.timestamp.time.hour as u32,
                        t.timestamp.time.minute as u32,
                        t.timestamp.time.second as u32,
                        t.timestamp.time.nanosecond,
                    )
                    .unwrap_or_default(),
                );
                let dt: DateTime<Utc> = DateTime::from_naive_utc_and_offset(ndt, Utc);
                dt.format("%Y-%m-%d %H:%M:%S %Z").to_string()
            }
            DbValue::Date(d) => NaiveDate::from_ymd_opt(d.year, d.month as u32, d.day as u32)
                .unwrap_or_default()
                .to_string(),
            DbValue::Time(t) => NaiveTime::from_hms_nano_opt(
                t.hour as u32,
                t.minute as u32,
                t.second as u32,
                t.nanosecond,
            )
            .unwrap_or_default()
            .to_string(),
            _ => String::new(),
        }
    }
}

impl FromDbValue for i16 {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Int2(i) => *i,
            DbValue::Int4(i) => *i as i16,
            DbValue::Int8(i) => *i as i16,
            _ => 0,
        }
    }
}

impl FromDbValue for i32 {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Int2(i) => *i as i32,
            DbValue::Int4(i) => *i,
            DbValue::Int8(i) => *i as i32,
            _ => 0,
        }
    }
}

impl FromDbValue for i64 {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Int2(i) => *i as i64,
            DbValue::Int4(i) => *i as i64,
            DbValue::Int8(i) => *i,
            _ => 0,
        }
    }
}

impl FromDbValue for f64 {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Float8(f) => *f,
            DbValue::Float4(f) => *f as f64,
            DbValue::Timestamp(t) => {
                let ndt = NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(t.date.year, t.date.month as u32, t.date.day as u32)
                        .unwrap_or_default(),
                    NaiveTime::from_hms_nano_opt(
                        t.time.hour as u32,
                        t.time.minute as u32,
                        t.time.second as u32,
                        t.time.nanosecond,
                    )
                    .unwrap_or_default(),
                );
                ndt.and_utc().timestamp() as f64
                    + (ndt.and_utc().timestamp_subsec_nanos() as f64 / 1e9)
            }
            DbValue::Timestamptz(t) => {
                let ndt = NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(
                        t.timestamp.date.year,
                        t.timestamp.date.month as u32,
                        t.timestamp.date.day as u32,
                    )
                    .unwrap_or_default(),
                    NaiveTime::from_hms_nano_opt(
                        t.timestamp.time.hour as u32,
                        t.timestamp.time.minute as u32,
                        t.timestamp.time.second as u32,
                        t.timestamp.time.nanosecond,
                    )
                    .unwrap_or_default(),
                );
                let dt: DateTime<Utc> = DateTime::from_naive_utc_and_offset(ndt, Utc);
                dt.timestamp() as f64 + (dt.timestamp_subsec_nanos() as f64 / 1e9)
            }
            DbValue::Date(d) => {
                let ndt = NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(d.year, d.month as u32, d.day as u32)
                        .unwrap_or_default(),
                    NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                );
                ndt.and_utc().timestamp() as f64
            }
            DbValue::Time(t) => {
                (t.hour as f64 * 3600.0)
                    + (t.minute as f64 * 60.0)
                    + (t.second as f64)
                    + (t.nanosecond as f64 / 1e9)
            }
            _ => 0.0,
        }
    }
}

impl FromDbValue for u64 {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Int8(i) => *i as u64,
            DbValue::Int4(i) => *i as u64,
            DbValue::Int2(i) => *i as u64,
            DbValue::Float8(f) => *f as u64,
            _ => 0,
        }
    }
}

impl FromDbValue for u8 {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Int2(i) => *i as u8,
            DbValue::Int4(i) => *i as u8,
            DbValue::Int8(i) => *i as u8,
            DbValue::Float8(f) => *f as u8,
            _ => 0,
        }
    }
}

impl FromDbValue for Option<String> {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Text(s) => Some(s.clone()),
            _ => None,
        }
    }
}

impl<T: FromDbValue> FromDbValue for Vec<T> {
    fn from_db_value(value: &DbValue) -> Self {
        match value {
            DbValue::Array(arr) => arr
                .iter()
                .map(|item| T::from_db_value(&item.get()))
                .collect(),
            _ => Vec::new(),
        }
    }
}

pub fn extract<T: FromDbValue>(value: &DbValue) -> T {
    T::from_db_value(value)
}
