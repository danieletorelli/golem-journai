mod database;

use common_lib::database::PostgresDatabase;
use common_lib::model::{APIError, APIErrorType, ServiceErrors};
use common_lib::Analyzer;
use database::Database;
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
        let model = env::var("JOURNAI_MODEL").map_err(|_| {
            APIErrorType::LLM.of_string("JOURNAI_MODEL env variable is not defined".to_string())
        })?;
        let entry_ids = errors.entries.clone();
        let entries = PostgresDatabase::get_entries(entry_ids)?;
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
            model: model.to_string(),
            temperature: Some(0.2),
            max_tokens: Some(131000),
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            provider_options: None,
        };

        log::debug!("Sending request to LLM...");
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

        let response = llm::send(&events, &config).map_err(|e| APIErrorType::LLM.of_llm(e))?;
        log::debug!("LLM Response: {:?}", response);

        let response_text = response
            .content
            .iter()
            .filter_map(|content_part| match content_part {
                llm::ContentPart::Text(txt) => Some(txt.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        PostgresDatabase::insert_analysis(
            self.hostname.to_string(),
            "spike".to_string(),
            model,
            response_text.clone(),
            errors.entries,
        )?;

        Ok(response_text)
    }
}

impl AnalyzerImpl {
    const SYSTEM_PROMPT: &str = r#"You are an SRE assistant.
        Answer concisely for on-call engineers, in plain English, and always include a short checklist of concrete actions."#;
    fn compose_user_prompt(
        &self,
        start: f64,
        end: f64,
        errors_count: u64,
        errors: String,
    ) -> String {
        format!(
            r#"Here is a set of systemd journal entries from host "{host}" for service "{service}"
            between timestamps {start} and {end}, grouped around an error spike of {errors_count} errors.

            1. "Summary" section: Explain in plain English what is likely happening, what changed, and which components are involved.
            2. "Causes" section: Suggest the most probable root cause or a small set of hypotheses.
            3. "Checklist" section: Output a short runbook-style checklist of concrete checks and actions.

            Errors:
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
