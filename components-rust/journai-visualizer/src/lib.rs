mod database;

use chrono::{DateTime, Utc};
use common_lib::model::APIError;
use common_lib::Visualizer;
use golem_rust::agent_implementation;
use std::time::SystemTime;

struct VisualizerImpl;

const HTML_HEADER: &str = r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>Dashboard</title>
<style>
body{font-family:Arial,sans-serif;margin:0;padding:20px;background:#f5f5f5}
.container{max-width:1200px;margin:0 auto}
.card{background:white;border-radius:8px;padding:20px;margin:10px 0;box-shadow:0 2px 4px rgba(0,0,0,0.1)}
.metric{display:inline-block;margin:10px 20px 10px 0}
.metric-value{font-size:2em;font-weight:bold;color:#2196F3}
.metric-label{color:#666;font-size:0.9em}
.alert{padding:10px;margin:5px 0;border-radius:4px}
.alert-critical{background:#ffebee;border-left:4px solid #f44336}
.alert-high{background:#fff3e0;border-left:4px solid #ff9800}
.alert-medium{background:#f3e5f5;border-left:4px solid #9c27b0}
.alert-low{background:#e8f5e8;border-left:4px solid #4caf50}
.queue-item{padding:8px;margin:4px 0;background:#f9f9f9;border-radius:4px}
.timestamp{color:#888;font-size:0.8em}
h1,h2{color:#333}
table{width:100%;border-collapse:collapse}
th,td{padding:8px;text-align:left;border-bottom:1px solid #ddd}
th{background:#f5f5f5}
</style>
</head><body><div class="container">
"#;

const HTML_FOOTER: &str = "</div></body></html>";

#[agent_implementation]
impl Visualizer for VisualizerImpl {
    fn new() -> Self {
        Self
    }

    fn dashboard_overview(&self) -> Result<String, APIError> {
        let overview = database::get_dashboard_overview()?;

        let hostnames_html = if overview.hostnames.is_empty() {
            "<p>No hosts found</p>".to_string()
        } else {
            let links = overview
                .hostnames
                .iter()
                .map(|h| format!(r#"<li><a href="/analysis/history/{}">{}</a></li>"#, h, h))
                .collect::<Vec<_>>()
                .join("\n");
            format!("<ul>{}</ul>", links)
        };

        let html = format!(
            r#"{}
<h1>Dashboard Overview</h1>
<div class="card">
    <h2>System Health</h2>
    <div class="metric">
        <div class="metric-value">{}</div>
        <div class="metric-label">Active Hosts</div>
    </div>
    <div class="metric">
        <div class="metric-value">{}</div>
        <div class="metric-label">Entries Today</div>
    </div>
    <div class="metric">
        <div class="metric-value">{}</div>
        <div class="metric-label">Error Spikes</div>
    </div>
    <div class="metric">
        <div class="metric-value">{}</div>
        <div class="metric-label">Critical Alerts</div>
    </div>
    <div class="metric">
        <div class="metric-value">{}/hr</div>
        <div class="metric-label">Collection Rate</div>
    </div>
</div>
<div class="card">
    <h2>Managed Hosts</h2>
    {}
</div>
<div class="card">
    <p>Last updated: {}</p>
    <p><a href="/dashboard/alerts">View Active Alerts</a> | <a href="/analysis/queue">Analysis Queue</a></p>
</div>
{}
            "#,
            HTML_HEADER,
            overview.active_hosts_count,
            overview.total_entries_today,
            overview.error_spikes_active,
            overview.critical_alerts,
            overview.collection_rate_per_hour,
            hostnames_html,
            DateTime::<Utc>::from(SystemTime::now()).format("%Y-%m-%d %H:%M:%S"),
            HTML_FOOTER
        );

        Ok(html)
    }

    fn dashboard_alerts(&self) -> Result<String, APIError> {
        let alerts = database::get_active_alerts()?;

        let alerts_html = if alerts.is_empty() {
            "<p>No active alerts</p>".to_string()
        } else {
            alerts.iter().map(|alert| {
                let alert_class = match alert.severity.as_str() {
                    "Critical" => "alert-critical",
                    "High" => "alert-high",
                    "Medium" => "alert-medium",
                    _ => "alert-low",
                };

                format!(
                    r#"<div class="alert {}">
                        <strong>{}/{}</strong> - {} errors
                        <div class="timestamp">Started: {} | Severity: {} | Action Required: {}</div>
                    </div>"#,
                    alert_class,
                    alert.hostname,
                    alert.service_name,
                    alert.error_count,
                    alert.started_at,
                    alert.severity,
                    if alert.needs_action { "Yes" } else { "No" }
                )
            }).collect::<Vec<_>>().join("\n")
        };

        let html = format!(
            r#"{}
<h1>Active Alerts</h1>
<div class="card">
    <h2>Critical & High Severity Issues</h2>
    {}
</div>
<div class="card">
    <p><a href="/dashboard/overview">Back to Overview</a></p>
</div>
{}
            "#,
            HTML_HEADER, alerts_html, HTML_FOOTER
        );

        Ok(html)
    }

    fn analysis_queue(&self) -> Result<String, APIError> {
        let queue = database::get_analysis_queue()?;

        let recent_html = if queue.recent_analyses.is_empty() {
            "<p>No recent analyses</p>".to_string()
        } else {
            queue
                .recent_analyses
                .iter()
                .map(|analysis| {
                    format!(
                        r#"<div class="queue-item">
                        <strong>{}/{}</strong> - {} ({})
                        <div class="timestamp">{}</div>
                    </div>"#,
                        analysis.service_name,
                        analysis.analysis_type,
                        analysis.severity,
                        analysis.hostname,
                        analysis.analysed_at
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let html = format!(
            r#"{}
<h1>Analysis Queue</h1>
<div class="card">
    <h2>Queue Status</h2>
    <div class="metric">
        <div class="metric-value">{}</div>
        <div class="metric-label">Pending Analyses</div>
    </div>
</div>
<div class="card">
    <h2>Recent Analyses</h2>
    {}
</div>
<div class="card">
    <p><a href="/dashboard/overview">Back to Overview</a></p>
</div>
{}
            "#,
            HTML_HEADER, queue.pending_analyses, recent_html, HTML_FOOTER
        );

        Ok(html)
    }

    fn analysis_history(&self, hostname: String) -> Result<String, APIError> {
        let history = database::get_analysis_history(hostname.clone())?;

        let history_html = if history.is_empty() {
            "<p>No analysis history found</p>".to_string()
        } else {
            let rows = history
                .iter()
                .map(|analysis| {
                    format!(
                        r#"<tr>
                            <td>{}</td>
                            <td>{}</td>
                            <td>{}</td>
                            <td>{}</td>
                            <td>{}</td>
                            <td>{}</td>
                            <td>{}</td>
                            <td>{}</td>
                            <td><a href="/analysis/details/{}">Details</a></td>
                        </tr>"#,
                        analysis.analysed_at,
                        analysis.service_name,
                        analysis.analysis_type,
                        analysis.severity,
                        analysis.entries_count,
                        analysis.model,
                        analysis.first_error,
                        analysis.last_error,
                        analysis.id
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                r#"<table>
                    <thead>
                        <tr>
                            <th>Date</th>
                            <th>Service</th>
                            <th>Type</th>
                            <th>Severity</th>
                            <th>Entries</th>
                            <th>Model</th>
                            <th>First Error</th>
                            <th>Last Error</th>
                            <th>Actions</th>
                        </tr>
                    </thead>
                    <tbody>{}</tbody>
                </table>"#,
                rows
            )
        };

        let html = format!(
            r#"{}
<h1>Analysis History - {}</h1>
<div class="card">
    <h2>Host Analyses</h2>
    {}
</div>
<div class="card">
    <p><a href="/dashboard/overview">Back to Overview</a></p>
</div>
{}
            "#,
            HTML_HEADER, hostname, history_html, HTML_FOOTER
        );

        Ok(html)
    }

    fn analysis_details(&self, analysis_id: String) -> Result<String, APIError> {
        let id = analysis_id.parse::<i32>().map_err(|_| {
            common_lib::model::APIErrorType::Fetch.of_string("Invalid analysis ID".to_string())
        })?;

        let details = database::get_analysis_details(id)?;

        let html = format!(
            r#"{}
<h1>Analysis Details - #{}</h1>
<div class="card">
    <h2>Overview</h2>
    <p><strong>Hostname:</strong> {}</p>
    <p><strong>Service:</strong> {}</p>
    <p><strong>Type:</strong> {}</p>
    <p><strong>Analysed At:</strong> {}</p>
    <p><strong>Severity:</strong> {}</p>
    <p><strong>Model Used:</strong> {}</p>
    <p><strong>Needs User Action:</strong> {}</p>
</div>

<div class="card">
    <h2>Analysis Result</h2>
    <p>{}</p>
</div>

<div class="card">
    <h2>Data Context</h2>
    <p><strong>Entries Count:</strong> {}</p>
    <p><strong>First Error:</strong> {}</p>
    <p><strong>Last Error:</strong> {}</p>
</div>

<div class="card">
    <p><a href="/analysis/history/{}">Back to Host History</a> | <a href="/dashboard/overview">Back to Overview</a></p>
</div>
{}
            "#,
            HTML_HEADER,
            details.id,
            details.hostname,
            details.service_name,
            details.analysis_type,
            details.analysed_at,
            details.severity,
            details.model,
            if details.needs_user_action {
                "Yes"
            } else {
                "No"
            },
            details.summary.replace("\n", "<br>"),
            details.entries_count,
            details.first_error,
            details.last_error,
            details.hostname,
            HTML_FOOTER
        );

        Ok(html)
    }
}
