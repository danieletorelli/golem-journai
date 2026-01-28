use common_lib::database::*;
use common_lib::model::{APIError, APIErrorType};
use golem_rust::bindings::golem::rdbms::postgres::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DashboardOverview {
    pub active_hosts_count: u64,
    pub total_entries_today: u64,
    pub error_spikes_active: u64,
    pub critical_alerts: u64,
    pub collection_rate_per_hour: u64,
    pub hostnames: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AlertInfo {
    pub hostname: String,
    pub service_name: String,
    pub severity: String,
    pub error_count: u64,
    pub started_at: String,
    pub needs_action: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueueInfo {
    pub pending_analyses: u64,
    pub recent_analyses: Vec<AnalysisInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalysisInfo {
    pub id: i32,
    pub hostname: String,
    pub service_name: String,
    pub analysis_type: String,
    pub analysed_at: String,
    pub severity: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DetailedAnalysisInfo {
    pub id: i32,
    pub hostname: String,
    pub service_name: String,
    pub analysis_type: String,
    pub analysed_at: String,
    pub severity: String,
    pub entries_count: i64,
    pub first_error: String,
    pub last_error: String,
    pub model: String,
    pub summary: String,
    pub needs_user_action: bool,
}

pub fn get_dashboard_overview() -> Result<DashboardOverview, APIError> {
    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let today_start = now - 86400.0;
    let recent_threshold = now - 300.0;
    let hour_ago = now - 3600.0;

    let active_hosts_count = execute_count_query(
        &conn,
        ACTIVE_HOSTS_COUNT_QUERY,
        vec![DbValue::Float8(recent_threshold)],
    )?;

    let hostnames = conn
        .query(ALL_HOSTNAMES_QUERY, vec![])
        .map(|result| {
            result
                .rows
                .iter()
                .map(|row| extract(&row.values[0]))
                .collect()
        })
        .map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

    let total_entries = execute_count_query(
        &conn,
        ENTRIES_TODAY_QUERY,
        vec![DbValue::Float8(today_start)],
    )?;
    let error_spikes = execute_count_query(
        &conn,
        ERROR_SPIKES_QUERY,
        vec![DbValue::Float8(recent_threshold)],
    )?;
    let critical_alerts = execute_count_query(&conn, CRITICAL_ALERTS_QUERY, vec![])?;
    let collection_rate = execute_count_query(
        &conn,
        COLLECTION_RATE_QUERY,
        vec![DbValue::Float8(hour_ago)],
    )?;

    Ok(DashboardOverview {
        active_hosts_count,
        total_entries_today: total_entries,
        error_spikes_active: error_spikes,
        critical_alerts,
        collection_rate_per_hour: collection_rate,
        hostnames,
    })
}

pub fn get_active_alerts() -> Result<Vec<AlertInfo>, APIError> {
    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

    if should_log_queries() {
        log::debug!("Query: {}", ACTIVE_ALERTS_QUERY);
    }

    conn.query(ACTIVE_ALERTS_QUERY, vec![])
        .map(|result| {
            result
                .rows
                .iter()
                .map(|row| {
                    let v = &row.values;
                    AlertInfo {
                        hostname: extract(&v[0]),
                        service_name: extract(&v[1]),
                        severity: extract(&v[2]),
                        error_count: extract(&v[3]),
                        started_at: extract(&v[4]),
                        needs_action: extract::<bool>(&v[5]),
                    }
                })
                .collect()
        })
        .map_err(|e| APIErrorType::Fetch.of_postgres(e))
}

pub fn get_analysis_queue() -> Result<QueueInfo, APIError> {
    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

    let hour_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        - 3600.0;

    let pending_count = execute_count_query(
        &conn,
        PENDING_ANALYSES_QUERY,
        vec![DbValue::Float8(hour_ago)],
    )?;

    if should_log_queries() {
        log::debug!("Query: {}", RECENT_ANALYSES_QUERY);
    }

    let recent_analyses = conn
        .query(RECENT_ANALYSES_QUERY, vec![])
        .map(|result| {
            result
                .rows
                .iter()
                .map(|row| {
                    let v = &row.values;
                    AnalysisInfo {
                        id: extract(&v[0]),
                        hostname: extract(&v[1]),
                        service_name: extract(&v[2]),
                        analysis_type: extract(&v[3]),
                        analysed_at: extract(&v[4]),
                        severity: extract(&v[5]),
                    }
                })
                .collect()
        })
        .map_err(|e| APIErrorType::Fetch.of_postgres(e))?;

    Ok(QueueInfo {
        pending_analyses: pending_count,
        recent_analyses,
    })
}

pub fn get_analysis_history(hostname: String) -> Result<Vec<DetailedAnalysisInfo>, APIError> {
    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;
    let params = vec![DbValue::Text(hostname)];

    if should_log_queries() {
        log::debug!("Query: {}", ANALYSIS_HISTORY_QUERY);
        log::trace!("Params: {:?}", params);
    }

    conn.query(ANALYSIS_HISTORY_QUERY, params)
        .map(|result| {
            result
                .rows
                .iter()
                .map(|row| {
                    let v = &row.values;
                    DetailedAnalysisInfo {
                        id: extract(&v[0]),
                        hostname: extract(&v[1]),
                        service_name: extract(&v[2]),
                        analysis_type: extract(&v[3]),
                        analysed_at: extract(&v[4]),
                        severity: extract(&v[5]),
                        entries_count: extract(&v[6]),
                        first_error: extract(&v[7]),
                        last_error: extract(&v[8]),
                        model: extract(&v[9]),
                        summary: extract(&v[10]),
                        needs_user_action: extract::<bool>(&v[11]),
                    }
                })
                .collect()
        })
        .map_err(|e| APIErrorType::Fetch.of_postgres(e))
}

pub fn get_analysis_details(analysis_id: i32) -> Result<DetailedAnalysisInfo, APIError> {
    let conn =
        PostgresDatabase::open_connection().map_err(|e| APIErrorType::Fetch.of_postgres(e))?;
    let params = vec![DbValue::Int4(analysis_id)];

    if should_log_queries() {
        log::debug!("Query: {}", ANALYSIS_DETAILS_QUERY);
        log::trace!("Params: {:?}", params);
    }

    conn.query(ANALYSIS_DETAILS_QUERY, params)
        .map_err(|e| APIErrorType::Fetch.of_postgres(e))?
        .rows
        .first()
        .map(|row| {
            let v = &row.values;
            DetailedAnalysisInfo {
                id: extract(&v[0]),
                hostname: extract(&v[1]),
                service_name: extract(&v[2]),
                analysis_type: extract(&v[3]),
                analysed_at: extract(&v[4]),
                severity: extract(&v[5]),
                entries_count: extract(&v[6]),
                first_error: extract(&v[7]),
                last_error: extract(&v[8]),
                model: extract(&v[9]),
                summary: extract(&v[10]),
                needs_user_action: extract::<bool>(&v[11]),
            }
        })
        .ok_or_else(|| {
            APIErrorType::Fetch.of_postgres(Error::Other("Analysis not found".to_string()))
        })
}

fn execute_count_query(
    conn: &DbConnection,
    query: &str,
    params: Vec<DbValue>,
) -> Result<u64, APIError> {
    if should_log_queries() {
        log::debug!("Query: {}", query);
        if !params.is_empty() {
            log::trace!("Params: {:?}", params);
        }
    }

    conn.query(query, params)
        .map(|result| {
            result
                .rows
                .first()
                .and_then(|row| row.values.first())
                .map(extract::<u64>)
                .unwrap_or(0)
        })
        .map_err(|e| APIErrorType::Fetch.of_postgres(e))
}

const ACTIVE_HOSTS_COUNT_QUERY: &str =
    "SELECT COUNT(DISTINCT hostname) FROM entries WHERE date > $1";
const ALL_HOSTNAMES_QUERY: &str = "SELECT DISTINCT hostname FROM entries ORDER BY hostname";
const ENTRIES_TODAY_QUERY: &str = "SELECT COUNT(*) FROM entries WHERE date > $1";
const ERROR_SPIKES_QUERY: &str = "SELECT COUNT(*) FROM entries WHERE date > $1 AND priority <= 3";
const CRITICAL_ALERTS_QUERY: &str = "SELECT COUNT(*) FROM analyses WHERE severity IN ('High', 'Critical') AND analysed_at > NOW() - INTERVAL '1 day'";
const COLLECTION_RATE_QUERY: &str = "SELECT COUNT(*) FROM entries WHERE date > $1";

const ACTIVE_ALERTS_QUERY: &str = r#"SELECT analyses.hostname,
       COALESCE(unit, syslog_identifier, comm, 'unknown') AS service_name,
       analyses.severity,
       COUNT(entry_id) as error_count,
       COALESCE(EXTRACT(EPOCH FROM MIN(TO_TIMESTAMP(date))), 0) as started_at,
       needs_user_action
FROM analyses
    JOIN analyzed_entries ON analyses.id = analyzed_entries.analysis_id
    JOIN entries ON analyzed_entries.entry_id = entries.id
WHERE analyses.severity IN ('High', 'Critical') AND analyses.analysed_at > NOW() - INTERVAL '1 hour'
GROUP BY analyses.id, analyses.hostname, analyses.severity, needs_user_action, service_name
ORDER BY analyses.analysed_at DESC
LIMIT 20"#;

const PENDING_ANALYSES_QUERY: &str =
    "SELECT COUNT(*) FROM entries WHERE priority <= 3 AND date > $1";

const RECENT_ANALYSES_QUERY: &str = r#"SELECT analyses.id,
       analyses.hostname,
       COALESCE(unit, syslog_identifier, comm, 'unknown') AS service_name,
       analyses.analysis_type,
       analyses.analysed_at,
       analyses.severity
FROM analyses
    JOIN analyzed_entries ON analyses.id = analyzed_entries.analysis_id
    JOIN entries ON analyzed_entries.entry_id = entries.id
WHERE analyses.analysed_at > NOW() - INTERVAL '1 hour'
GROUP BY analyses.id, analyses.hostname, analyses.analysis_type, analyses.analysed_at, analyses.severity, service_name
ORDER BY analyses.analysed_at DESC
LIMIT 10"#;

const ANALYSIS_HISTORY_QUERY: &str = r#"SELECT analyses.id,
       analyses.hostname,
       COALESCE(unit, syslog_identifier, comm, 'unknown') AS service_name,
       analyses.analysis_type,
       analyses.analysed_at,
       analyses.severity,
       COUNT(entry_id) AS entries_count,
       MIN(TO_TIMESTAMP(date)) AS first_error,
       MAX(TO_TIMESTAMP(date)) AS last_error,
       analyses.model,
       analyses.summary,
       analyses.needs_user_action
FROM analyses
    JOIN analyzed_entries ON analyses.id = analyzed_entries.analysis_id
    JOIN entries ON analyzed_entries.entry_id = entries.id
WHERE analyses.hostname = $1
GROUP BY analyses.id, analyses.hostname, analyses.analysis_type, analyses.analysed_at, analyses.severity, service_name
ORDER BY analyses.severity ASC, entries_count DESC, last_error DESC
LIMIT 50"#;

const ANALYSIS_DETAILS_QUERY: &str = r#"SELECT analyses.id,
       analyses.hostname,
       COALESCE(unit, syslog_identifier, comm, 'unknown') AS service_name,
       analyses.analysis_type,
       analyses.analysed_at,
       analyses.severity,
       COUNT(entry_id) AS entries_count,
       MIN(TO_TIMESTAMP(date)) AS first_error,
       MAX(TO_TIMESTAMP(date)) AS last_error,
       analyses.model,
       analyses.summary,
       analyses.needs_user_action
FROM analyses
    JOIN analyzed_entries ON analyses.id = analyzed_entries.analysis_id
    JOIN entries ON analyzed_entries.entry_id = entries.id
WHERE analyses.id = $1
GROUP BY analyses.id, analyses.hostname, analyses.analysis_type, analyses.analysed_at, analyses.severity, service_name"#;
