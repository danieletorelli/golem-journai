mod database;

use common_lib::model::{APIError, APIErrorType, ServiceErrors, SpikeEventAssertion};
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

        let (mut events, spike_event_summary) =
            self.execute_spike_summary_llm_call(&model, &errors)?;
        let spike_event_assertion = self.execute_spike_structured_llm_call(&model, &mut events)?;

        if !spike_event_summary.trim().is_empty() {
            database::insert_analysis(
                self.hostname.clone(),
                "spike".to_string(),
                model,
                spike_event_summary.clone(),
                spike_event_assertion,
                errors.entries,
            )?;
            Ok(spike_event_summary)
        } else {
            log::error!("Error: Analysis response was empty");
            Err(APIErrorType::LLM.of_string("Analysis response was empty".to_string()))
        }
    }
}

impl AnalyzerImpl {
    const LLM_ENTRIES_LIMIT_DEFAULT: u16 = 500;
    const SPIKE_SUMMARY_SYSTEM_PROMPT: &str = r#"You are an expert Senior Site Reliability Engineer (SRE).
        Your task is to analyze systemd journal entries to identify the root cause of service failures or error spikes.
        Provide a professional, technical, and concise analysis for on-call engineers.
        Focus on identifying patterns, specific error codes, and sequence of events.
        Convert to human readable timestamps the raw seconds from epoch that you find.
        Always conclude with a 'Next Steps' checklist of concrete, actionable troubleshooting steps."#;
    const SPIKE_STRUCTURED_SYSTEM_PROMPT: &str = r#"You are an expert Senior Site Reliability Engineer (SRE).
        Your task is to analyze systemd journal entries to identify the root cause of service failures or error spikes.
        Provide a structured response in compact (no spaces, new lines and so on) JSON format.
        Do not include any additional text, formatting or explanations.
        This is VERY IMPORTANT as I need to parse this response and I need a strict JSON."#;
    const SPIKE_STRUCTURED_USER_PROMPT: &str = "How critical is this error spike?";

    fn compose_spike_summary_user_prompt(
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

    fn execute_spike_summary_llm_call(
        &self,
        model: &str,
        errors: &ServiceErrors,
    ) -> Result<(Vec<llm::Event>, String), APIError> {
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

        let user_prompt = self.compose_spike_summary_user_prompt(
            errors.started_at,
            errors.last_at,
            errors.error_count,
            entries_json,
        );

        let config = llm::Config {
            model: model.to_string(),
            temperature: Some(0.2),
            max_tokens: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            provider_options: None,
        };

        log::debug!("Sending request to LLM ({:?})", config);
        let mut events = vec![
            llm::Event::Message(llm::Message {
                role: llm::Role::System,
                name: None,
                content: vec![llm::ContentPart::Text(
                    Self::SPIKE_SUMMARY_SYSTEM_PROMPT.to_string(),
                )],
            }),
            llm::Event::Message(llm::Message {
                role: llm::Role::User,
                name: Some("JournAI".to_string()),
                content: vec![llm::ContentPart::Text(user_prompt)],
            }),
        ];

        let response = llm::send(&events, &config)
            .map(|r| {
                log::debug!("LLM Response: {:?}", r);
                events.push(llm::Event::Response(r.clone()));
                r
            })
            .map_err(|e| {
                log::error!("Error: {:?}", e);
                APIErrorType::LLM.of_llm(e)
            })?;

        let response_text = response
            .content
            .iter()
            .filter_map(|content_part| match content_part {
                llm::ContentPart::Text(txt) if !txt.trim().is_empty() => Some(txt.trim()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok((events, response_text))
    }

    fn execute_spike_structured_llm_call(
        &self,
        model: &str,
        events: &mut Vec<llm::Event>,
    ) -> Result<SpikeEventAssertion, APIError> {
        let response_format = serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": "event_assertion",
                "strict": true,
                "schema": serde_json::json!({
            "type": "object",
            "properties": {
                "severity": {
                    "type": "string",
                    "description": "The severity of the error spike. It's critical only if the system can't start without fixing the issue. It's high only if it needs to be addressed quickly or it will create problems in the immediate future. It's low if it can be ignored or handled later, especially if the errors are happening from a while. Otherwise, it's medium.",
                    "enum": ["Low", "Medium", "High", "Critical"]
                },
                "needs_user_action": {
                    "type": "boolean",
                    "description": "Indicates if the error spike requires immediate user intervention."
                }
            },
            "required": ["severity", "needs_user_action"],
            "additionalProperties": false
        })
            }
        });

        let config = llm::Config {
            model: model.to_string(),
            temperature: Some(0.2),
            max_tokens: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            provider_options: None,
        };

        log::debug!("Sending request to LLM ({:?})", config);
        events.retain(|event| matches!(event, llm::Event::Response(_)));
        events.extend(vec![
            llm::Event::Message(llm::Message {
                role: llm::Role::System,
                name: None,
                content: vec![llm::ContentPart::Text(format!(
                    "{}\n{}",
                    Self::SPIKE_STRUCTURED_SYSTEM_PROMPT,
                    response_format
                ))],
            }),
            llm::Event::Message(llm::Message {
                role: llm::Role::User,
                name: Some("JournAI".to_string()),
                content: vec![llm::ContentPart::Text(
                    Self::SPIKE_STRUCTURED_USER_PROMPT.to_string(),
                )],
            }),
        ]);

        let response = llm::send(events, &config)
            .map(|r| {
                log::debug!("LLM Response: {:?}", r);
                r
            })
            .map_err(|e| {
                log::error!("Error: {:?}", e);
                APIErrorType::LLM.of_llm(e)
            })?;

        let assertion = serde_json::from_str(
            &response
                .content
                .iter()
                .filter_map(|content_part| match content_part {
                    llm::ContentPart::Text(txt) if !txt.trim().is_empty() => Some(txt.trim()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
                .replace("```json", "")
                .replace("```", "")
                .trim()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
        )
        .map_err(|e| {
            log::error!("Error parsing LLM response: {:?}\nError: {:?}", response, e);
            APIError::Other(format!("Error parsing LLM response: {:?}", e))
        })?;

        Ok(assertion)
    }
}
