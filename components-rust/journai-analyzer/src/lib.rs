mod database;

use common_lib::model::{APIError, APIErrorType, JournalEntry, ServiceErrors, SpikeEventAssertion};
use common_lib::Analyzer;
use golem_rust::agent_implementation;
use golem_rust::golem_ai::golem::llm::llm;
use std::env;

struct AnalyzerImpl {
    hostname: String,
    service: String,
    events: Vec<llm::Event>,
}

#[agent_implementation]
impl Analyzer for AnalyzerImpl {
    fn new(hostname: String, service: String) -> Self {
        Self {
            hostname,
            service,
            events: vec![],
        }
    }

    async fn analyze_spike(&mut self, errors: ServiceErrors) -> Result<String, APIError> {
        let model = env::var("JOURNAI_LLM_MODEL").map_err(|_| {
            APIErrorType::LLM.of_string("JOURNAI_LLM_MODEL env variable is not defined".to_string())
        })?;

        let (events, spike_event_summary) = self.execute_spike_summary_llm_call(&model, &errors)?;
        let spike_event_assertion = self.execute_spike_structured_llm_call(&model, &events)?;

        self.events = self.compact_events(events);

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
    const LLM_ENTRIES_LIMIT: u16 = 500;
    const LLM_CONTEXT_WINDOW_LIMIT: usize = 20;
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

    fn compose_analysis_request_header(&self, start: f64, end: f64, errors_count: u64) -> String {
        format!(
            r#"Analysis Request for host: "{host}", service: "{service}"
            Time Range: {start} to {end} (Unix timestamps)
            Incident Profile: Detected a spike of {errors_count} errors in this window."#,
            host = self.hostname,
            service = self.service,
            start = start,
            end = end,
            errors_count = errors_count,
        )
    }

    fn compose_spike_summary_user_prompt(
        &self,
        errors: &ServiceErrors,
        entries: &[JournalEntry],
    ) -> String {
        let header = self.compose_analysis_request_header(
            errors.started_at,
            errors.last_at,
            errors.error_count,
        );

        let entries_json = entries
            .iter()
            .filter_map(|entry| {
                serde_json::to_string(entry)
                    .map_err(|e| {
                        log::warn!("Failed to serialize entry: {}", e);
                        e
                    })
                    .ok()
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(
            r#"{header}

            Please analyze the following journal entries and provide:
            1. **Summary**: A high-level overview of the incident (what happened and impact).
            2. **Technical Deep-Dive**: Identify specific error patterns, failed assertions, or timeout signals. Mention specific PIDs or file paths if relevant.
            3. **Probable Root Cause**: Your best hypothesis for why this happened (e.g., resource exhaustion, configuration error, upstream dependency failure).
            4. **Checklist of Actions**: 3-5 concrete commands or checks for the on-call engineer to run immediately.

            Journal Entries (JSON format):
            {entries_json}"#,
            header = header,
            entries_json = entries_json,
        )
    }

    fn compose_spike_summary_user_prompt_lite(
        &self,
        errors: &ServiceErrors,
        entries: &[JournalEntry],
    ) -> String {
        let header = self.compose_analysis_request_header(
            errors.started_at,
            errors.last_at,
            errors.error_count,
        );

        let mut unique_messages = Vec::new();
        for entry in entries {
            let msg = if entry.message.len() > 200 {
                format!("{}...", &entry.message[..200])
            } else {
                entry.message.clone()
            };
            if !unique_messages.contains(&msg) {
                unique_messages.push(msg);
            }
            if unique_messages.len() >= 5 {
                break;
            }
        }
        let error_sample = unique_messages.join("\n");

        format!(
            r#"{header}
            Error Sample (first unique messages):\n{error_sample}"#,
            header = header,
            error_sample = error_sample
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
            .unwrap_or_else(|| {
                log::warn!(
                    "Invalid or missing JOURNAI_LLM_ENTRIES_LIMIT, using default: {}",
                    Self::LLM_ENTRIES_LIMIT
                );
                Self::LLM_ENTRIES_LIMIT
            });

        let entries = database::get_entries_by_ids(errors.entries.clone(), entries_limit)?;

        let user_prompt = self.compose_spike_summary_user_prompt(errors, &entries);
        let lite_user_prompt = self.compose_spike_summary_user_prompt_lite(errors, &entries);

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

        let system_message = llm::Message {
            role: llm::Role::System,
            name: None,
            content: vec![llm::ContentPart::Text(
                Self::SPIKE_SUMMARY_SYSTEM_PROMPT.to_string(),
            )],
        };

        let previous_context: Vec<llm::Event> = self
            .events
            .iter()
            .filter(|event| match event {
                llm::Event::Message(msg) => msg.role != llm::Role::System,
                _ => true,
            })
            .cloned()
            .collect();

        let mut call_events = vec![llm::Event::Message(system_message)];
        call_events.append(&mut previous_context.clone());
        call_events.push(llm::Event::Message(llm::Message {
            role: llm::Role::User,
            name: Some("JournAI".to_string()),
            content: vec![llm::ContentPart::Text(user_prompt)],
        }));

        let response = llm::send(&call_events, &config)
            .map(|r| {
                log::debug!("LLM Response: {:?}", r);
                r
            })
            .map_err(|e| {
                log::error!("LLM Request Failed: {:?}", e);
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

        if response_text.is_empty() {
            return Err(APIError::LLMError(
                "LLM returned empty response".to_string(),
            ));
        }

        // Build history-optimized events: Previous context + Lite User + Response
        let mut history_events = previous_context;
        history_events.push(llm::Event::Message(llm::Message {
            role: llm::Role::User,
            name: Some("JournAI".to_string()),
            content: vec![llm::ContentPart::Text(lite_user_prompt)],
        }));
        history_events.push(llm::Event::Response(response));

        Ok((history_events, response_text))
    }

    fn execute_spike_structured_llm_call(
        &self,
        model: &str,
        events: &[llm::Event],
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
                    "description": "The severity of the error spike. It's critical only if the system can't start without fixing the issue or if it's caused by a probable hardware issue. It's high only if it needs to be addressed quickly or it will create problems in the immediate future. It's low if it can be ignored or handled later, especially if the errors are happening from a while. Otherwise, it's medium.",
                    "enum": ["Low", "Medium", "High", "Critical"]
                },
                "needs_user_action": {
                    "type": "boolean",
                    "description": "It's true if the error spike requires immediate user intervention and a notification should be sent; false otherwise."
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

        // Optimization: For structured analysis, we only need the conversation history (Summaries and Lite User info)
        // and the current specific instructions. We avoid previous System prompts to reduce tokens and confusion.
        let mut request_events: Vec<llm::Event> = vec![llm::Event::Message(llm::Message {
            role: llm::Role::System,
            name: None,
            content: vec![llm::ContentPart::Text(format!(
                "{}\n{}",
                Self::SPIKE_STRUCTURED_SYSTEM_PROMPT,
                response_format
            ))],
        })];

        let mut conversation_history: Vec<llm::Event> = events
            .iter()
            .filter(|event| {
                matches!(event, llm::Event::Response(_))
                    || matches!(event, llm::Event::Message(m) if m.role == llm::Role::User)
            })
            .cloned()
            .collect();

        request_events.append(&mut conversation_history);
        request_events.push(llm::Event::Message(llm::Message {
            role: llm::Role::User,
            name: Some("JournAI".to_string()),
            content: vec![llm::ContentPart::Text(
                Self::SPIKE_STRUCTURED_USER_PROMPT.to_string(),
            )],
        }));

        let response = llm::send(&request_events, &config)
            .map(|r| {
                log::debug!("LLM Response: {:?}", r);
                r
            })
            .map_err(|e| {
                log::error!("LLM structured call failed: {:?}", e);
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
            .join("");

        let json_value: serde_json::Value = serde_json::from_str(&response_text)
            .or_else(|_| {
                serde_json::from_str(
                    response_text
                        .replace("```json", "")
                        .replace("```", "")
                        .trim(),
                )
            })
            .map_err(|e| {
                log::error!(
                    "Failed to parse LLM response as JSON: {}\nResponse: {}",
                    e,
                    response_text
                );
                APIError::Other(format!("Error parsing LLM response: {}", e))
            })?;

        let assertion: SpikeEventAssertion = serde_json::from_value(json_value).map_err(|e| {
            log::error!(
                "LLM response failed schema validation: {}\nResponse: {}",
                e,
                response_text
            );
            APIError::Other(format!("Invalid schema in LLM response: {}", e))
        })?;

        Ok(assertion)
    }

    fn compact_events(&self, events: Vec<llm::Event>) -> Vec<llm::Event> {
        let context_window_limit: usize = env::var("JOURNAI_LLM_CONTEXT_WINDOW_LIMIT")
            .ok()
            .and_then(|limit| limit.parse().ok())
            .unwrap_or_else(|| {
                log::warn!(
                    "Invalid or missing JOURNAI_LLM_CONTEXT_WINDOW_LIMIT, using default: {}",
                    Self::LLM_CONTEXT_WINDOW_LIMIT
                );
                Self::LLM_CONTEXT_WINDOW_LIMIT
            });

        let mut all_events = events;
        if all_events.len() > context_window_limit {
            let start_index = all_events.len() - context_window_limit;
            all_events = all_events.drain(start_index..).collect();
        }
        all_events
    }
}
