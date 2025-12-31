mod database;

use common_lib::model::{APIError, APIErrorType, ServiceErrors};
use common_lib::Analyzer;
use golem_rust::agent_implementation;
use golem_rust::golem_ai::golem::llm::llm;
use std::env;

struct AnalyzerImpl {
    hostname: String,
    service: String,
}

#[agent_implementation]
impl Analyzer for AnalyzerImpl {
    fn new(hostname: String, service: String) -> Self {
        Self { hostname, service }
    }

    async fn analyze_spike(&self, errors: ServiceErrors) -> Result<String, APIError> {
        let model = env::var("JOURNAI_LLM_MODEL").map_err(|_| {
            APIErrorType::LLM.of_string("JOURNAI_LLM_MODEL env variable is not defined".to_string())
        })?;
        let entries_limit: u16 = env::var("JOURNAI_LLM_ENTRIES_LIMIT")
            .ok()
            .and_then(|limit| limit.parse().ok())
            .unwrap_or(Self::LLM_ENTRIES_LIMIT_DEFAULT);

        let entries = database::get_entries_by_ids(errors.entries.clone(), entries_limit)?;
        let entries_json = entries
            .iter()
            .filter_map(|entry| serde_json::to_string(entry).ok())
            .collect::<Vec<_>>()
            .join("\n\n");

        let user_prompt = self.compose_user_prompt(
            errors.started_at,
            errors.last_at,
            errors.error_count,
            entries_json,
        );

        let config = llm::Config {
            model: model.clone(),
            temperature: Some(0.2),
            max_tokens: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            provider_options: Some(vec![llm::Kv {
                key: "transformers".to_string(),
                value: serde_json::to_value(vec!["middle-out"])
                    .unwrap()
                    .to_string(),
            }]),
        };

        log::debug!("Sending request to LLM ({:?})", config);
        let events = vec![
            llm::Event::Message(llm::Message {
                role: llm::Role::System,
                name: None,
                content: vec![llm::ContentPart::Text(Self::SYSTEM_PROMPT.to_string())],
            }),
            llm::Event::Message(llm::Message {
                role: llm::Role::User,
                name: Some("JournAI".to_string()),
                content: vec![llm::ContentPart::Text(user_prompt)],
            }),
        ];

        let response = llm::send(&events, &config).map_err(|e| {
            log::error!("Error: {:?}", e);
            APIErrorType::LLM.of_llm(e)
        })?;

        let response_text: String = response
            .content
            .iter()
            .filter_map(|content_part| match content_part {
                llm::ContentPart::Text(txt) if !txt.is_empty() => Some(txt.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if !response_text.is_empty() {
            log::debug!("LLM Response: {:?}", response);
            database::insert_analysis(
                self.hostname.clone(),
                "spike".to_string(),
                model,
                response_text.clone(),
                errors.entries,
            )?;
            Ok(response_text)
        } else {
            log::error!("Error: Analysis response was empty");
            Err(APIErrorType::LLM.of_string("Analysis response was empty".to_string()))
        }
    }
}

impl AnalyzerImpl {
    const LLM_ENTRIES_LIMIT_DEFAULT: u16 = 500;
    const SYSTEM_PROMPT: &str = r#"You are an expert Senior Site Reliability Engineer (SRE).
        Your task is to analyze systemd journal entries to identify the root cause of service failures or error spikes.
        Provide a professional, technical, and concise analysis for on-call engineers.
        Focus on identifying patterns, specific error codes, and sequence of events.
        Always conclude with a 'Next Steps' checklist of concrete, actionable troubleshooting steps."#;
    fn compose_user_prompt(
        &self,
        start: f64,
        end: f64,
        errors_count: u64,
        errors: String,
    ) -> String {
        format!(
            r#"Analysis Request for host: "{host}", service: "{service}"
            Time Range: {start} to {end} (Unix timestamps)
            Incident Profile: Detected a spike of {errors_count} errors in this window.

            Please analyze the following journal entries and provide:
            1. **Summary**: A high-level overview of the incident (what happened and impact).
            2. **Technical Deep-Dive**: Identify specific error patterns, failed assertions, or timeout signals. Mention specific PIDs or file paths if relevant.
            3. **Probable Root Cause**: Your best hypothesis for why this happened (e.g., resource exhaustion, configuration error, upstream dependency failure).
            4. **Checklist of Actions**: 3-5 concrete commands or checks for the on-call engineer to run immediately.

            Journal Entries (JSON format):
            {errors}"#,
            host = self.hostname,
            service = self.service,
            start = start,
            end = end,
            errors_count = errors_count,
            errors = errors,
        )
    }
}
