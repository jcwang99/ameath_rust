use crate::ai::client::{Content, ContentPart, ImageUrl, Message, OpenAiClient};
use crate::ai::memory::MemoryManager;
use crate::ai::skills::SkillManager;
use crate::types::{AiConfig, AiResponseEvent, ThinkingState};
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub struct ChatKernel {
    config: AiConfig,
    client: Option<OpenAiClient>,
    memory: Arc<MemoryManager>,
    skills: SkillManager,
}

impl ChatKernel {
    pub fn new(config: &AiConfig, scheduler: crate::interaction::ActionScheduler) -> Self {
        let profile = config.active_profile();
        let client = if profile.api_key.is_empty() {
            None
        } else {
            Some(OpenAiClient::new(
                profile.api_key.clone(),
                profile.base_url.clone(),
                profile.model.clone(),
                profile.response_mode,
            ))
        };

        let memory = Arc::new(MemoryManager::new());
        let skills = SkillManager::new(Arc::clone(&memory), config, scheduler);

        Self {
            config: config.clone(),
            client,
            memory,
            skills,
        }
    }
    pub fn get_recent_history(&self, limit: usize) -> Result<Vec<(String, String)>, String> {
        self.memory
            .get_recent_history(limit)
            .map_err(|e| e.to_string())
    }

    pub async fn handle(&self, input_data: crate::types::ChatInput, tx: Sender<AiResponseEvent>) {
        let input = input_data.text;
        let images = input_data.images;
        let client = match &self.client {
            Some(c) => c,
            None => {
                let _ = tx.send(AiResponseEvent::Response(
                    "Please configure your AI settings first!".to_string(),
                ));
                return;
            }
        };

        // --- #log FAST-TRACK ---
        if input.trim_start().starts_with("#log ") {
            let log_content = input.trim_start()["#log ".len()..].trim();
            if !log_content.is_empty() {
                match crate::ai::skills::work_log::WorkLogSkill::record_log_local(log_content) {
                    Ok(_) => {
                        let confirmation = "好哒，已经帮你记在小本本上啦~ [IMG]assets/stickers/写笔记.gif";
                        
                        // Persistent persistence for the command and response
                        self.memory.add_conversation_item("user", &input, 1).ok();
                        self.memory.add_conversation_item("assistant", confirmation, 1).ok();
                        
                        let _ = tx.send(AiResponseEvent::Response(confirmation.to_string()));
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(AiResponseEvent::Response(format!("日志记录失败: {}", e)));
                        return;
                    }
                }
            }
        }
        // -----------------------

        // 1. Initial User Message
        // Clear volatile tool traces from previous sessions/requests
        self.memory.clear_traces().ok();

        let (db_content, llm_content) = if let Some(idx) = input.find("\n\n[SYSTEM INSTRUCTION]") {
            (input[..idx].to_string(), input.clone())
        } else {
            (input.clone(), input.clone())
        };

        let mut parts = vec![ContentPart::Text { text: llm_content }];

        for img in images {
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &img.data);
            parts.push(ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: format!("data:{};base64,{}", img.mime_type, b64),
                },
            });
        }

        let user_msg_content = if parts.len() > 1 {
            Content::Multimodal(parts)
        } else {
            Content::Simple(db_content.clone())
        };

        let user_msg = Message {
            role: "user".to_string(),
            content: Some(user_msg_content),
            tool_calls: None,
            tool_call_id: None,
        };
        // DEFER SAVING until we have a response
        // self.memory.add_message(&user_msg).ok();

        // 2. ReAct Loop (Infinite with Handover)
        let mut total_handovers = 0;
        const MAX_HANDOVERS: usize = 3;
        let mut handover_context: Option<String> = None;

        loop {
            // Refresh context from memory (picks up new summaries and cleared traces)
            let mut messages = self
                .memory
                .get_context(self.config.l1_summary_threshold)
                .unwrap_or_default();

            // Inject Base System Prompt (Configurable Persona)
            if !self.config.system_prompt.is_empty() {
                messages.insert(
                    0,
                    Message {
                        role: "system".to_string(),
                        content: Some(Content::Simple(self.config.system_prompt.clone())),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                );
            }

            // Inject in-memory handover context if available
            if let Some(ctx) = &handover_context {
                messages.push(Message {
                    role: "system".to_string(),
                    content: Some(Content::Simple(ctx.clone())),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }

            // INJECT CURRENT USER MESSAGE (Deferred Persistence Fix)
            messages.push(user_msg.clone());
            tracing::debug!("Context prepared. Message count: {}", messages.len());

            let tools = self.skills.get_tools_for_llm();
            let tools_opt = if tools.is_empty() {
                tracing::info!("No tools available.");
                None
            } else {
                tracing::info!("Tools available: {}", tools.len());
                Some(tools)
            };

            let mut turns = 0;
            let max_turns = self.config.react_limit;
            let profile = self.config.active_profile();
            tracing::info!(
                "Starting ReAct loop with Profile: {} | Context: {} msgs | Limit: {} turns",
                profile.name,
                messages.len(),
                max_turns
            );

            let mut final_response = None;
            let mut fc_traces = Vec::new();

            while turns < max_turns {
                turns += 1;
                tracing::info!("Turn {}/{}", turns, max_turns);
                let _ = tx.send(AiResponseEvent::Status(ThinkingState::Network));
                
                tracing::debug!("Requesting LLM with {} messages", messages.len());
                let response_result = client.chat(messages.clone(), tools_opt.clone()).await;
                let _ = tx.send(AiResponseEvent::Status(ThinkingState::Standard));

                match response_result {
                    Ok(response_msg) => {
                        let content_str = response_msg.content_as_str();
                        let content_preview = if content_str.chars().count() > 100 {
                            format!("{}...", content_str.chars().take(100).collect::<String>())
                        } else {
                            content_str.clone()
                        };
                        
                        tracing::info!("LLM Response Received | Role: {} | Content: \"{}\"", 
                            response_msg.role, 
                            content_preview.replace("\n", " ")
                        );
                        if let Some(calls) = &response_msg.tool_calls {
                            if !calls.is_empty() {
                                tracing::info!("Tool Calls detected: {}", calls.len());
                            }
                        }

                        messages.push(response_msg.clone());

                        if let Some(tool_calls) = &response_msg.tool_calls {
                            if tool_calls.is_empty() {
                                tracing::info!("Tool calls field is present but empty. Breaking loop as final response.");
                                final_response = Some(response_msg.clone());
                                break; // Break inner loop
                            }

                            tracing::info!("Processing {} tool calls...", tool_calls.len());

                            // Execution Phase
                            for tool_call in tool_calls {
                                let skill_name = &tool_call.function.name;
                                let args_str = &tool_call.function.arguments;
                                tracing::info!(
                                    "Executing Tool: {} with args: {}",
                                    skill_name,
                                    args_str
                                );
                                let _ = tx.send(AiResponseEvent::Status(ThinkingState::Tools));

                                let args_result: Result<serde_json::Value, _> =
                                    serde_json::from_str(args_str);
                                let args = args_result.unwrap_or_else(|e| {
                                    tracing::error!("JSON Parse Error: {}", e);
                                    serde_json::json!({})
                                });

                                let result = if let Some(skill) = self.skills.get(skill_name) {
                                    match skill.execute(args).await {
                                        Ok(out) => {
                                            tracing::debug!("Tool Output: {:.100}...", out);
                                            out
                                        }
                                        Err(err) => {
                                            let e = format!("Error: {}", err);
                                            tracing::error!("Tool Execution Failed: {}", e);
                                            e
                                        }
                                    }
                                } else {
                                    let e = format!("Unknown tool: {}", skill_name);
                                    tracing::error!("{}", e);
                                    e
                                };

                                fc_traces.push(format!(
                                    "Tool: {}\nArgs: {}\nOutput: {}",
                                    skill_name, args_str, result
                                ));

                                let tool_response = Message {
                                    role: "tool".to_string(),
                                    content: Some(Content::Simple(result)),
                                    tool_calls: None,
                                    tool_call_id: Some(tool_call.id.clone()),
                                };

                                // Store in Layer 2 (Traces)
                                self.memory.add_trace(&response_msg).ok();
                                self.memory.add_trace(&tool_response).ok();

                                messages.push(tool_response);
                            }
                            let _ = tx.send(AiResponseEvent::Status(ThinkingState::Standard));
                        } else {
                            tracing::info!(
                                "No tool calls. Final response received. ({} turns)",
                                turns
                            );
                            final_response = Some(response_msg.clone());
                            break; // Break inner loop
                        }
                    }
                    Err(e) => {
                        tracing::error!("AI Client Error: {}", e);
                        let _ = tx.send(AiResponseEvent::Response(format!("AI Error: {}", e)));
                        return;
                    }
                }
            }

            if let Some(mut response_msg) = final_response {
                let original_content = response_msg.content_as_str();
                let display_content = crate::ai::memory::MemoryManager::strip_auxiliary_for_display(
                    &original_content,
                );
                
                // 1. Process FC Traces
                let fc_summary = if !fc_traces.is_empty() {
                    let mut combined_traces = fc_traces.join("\n---\n");
                    if combined_traces.len() > 2500 {
                        tracing::info!("FC traces too long ({} chars), summarizing...", combined_traces.len());
                        let summary_prompt = vec![Message {
                            role: "user".to_string(),
                            content: Some(Content::Simple(format!(
                                "Please summarize the following tool calling process into a concise log within 1000 tokens. \
                                Focus on what tools were called and what key information was obtained.\n\n[Tool Traces]:\n{}",
                                combined_traces
                            ))),
                            tool_calls: None,
                            tool_call_id: None,
                        }];
                        if let Ok(summary_msg) = client.chat(summary_prompt, None).await {
                            combined_traces = summary_msg.content_as_str();
                        }
                    }
                    Some(combined_traces)
                } else {
                    None
                };

                // 2. Prepare content for DB (keep FC log for future context)
                let db_content = if let Some(traces) = fc_summary {
                    format!("{}\n\n--- FC 调用过程记录 ---\n{}", original_content, traces)
                } else {
                    original_content.clone()
                };

                // 3. Save dialogue to Memory (L1)
                self.memory.add_message(&user_msg).ok();
                response_msg.content = Some(Content::Simple(db_content));
                self.memory.add_message(&response_msg).ok();

                // 4. Send to UI (Original content only)
                let _ = tx.send(AiResponseEvent::StreamEnd(display_content));

                // 5. Orchestrate summarization (L1 -> L2)
                let kernel_clone = Arc::new(Self {
                    config: self.config.clone(),
                    client: self.client.clone(),
                    memory: Arc::clone(&self.memory),
                    skills: self.skills.clone(),
                });
                tokio::spawn(async move {
                    kernel_clone.orchestrate_summarization().await.ok();
                });
                return;
            }

            // If we are here, we hit max_turns without a final response.
            // Cognitive Handover
            total_handovers += 1;
            if total_handovers >= MAX_HANDOVERS {
                let _ = tx.send(AiResponseEvent::Response("Cognitive Limit Reached (Max Handovers). Terminating to prevent infinite loop.".to_string()));
                return;
            }

            // Capture recent tool traces (last 5) from current messages BEFORE generating summary prompt
            let trace_capture_len = 5;
            let captured_traces = messages
                .iter()
                .rev()
                .filter(|m| m.role == "tool" || (m.role == "assistant" && m.tool_calls.is_some()))
                .take(trace_capture_len)
                .map(|m| {
                    let content_str = m.content_as_str();
                    let content_preview = if content_str.chars().count() > 200 {
                        format!("{}...", content_str.chars().take(200).collect::<String>())
                    } else {
                        content_str.to_string()
                    };
                    format!("[{}] {}", m.role, content_preview)
                })
                .collect::<Vec<_>>();
            // Reverse back to chronological order
            let recent_history_str = captured_traces
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");

            // Generate Handover Summary
            let summary_prompt = Message {
                role: "system".to_string(),
                content: Some(Content::Simple("You have reached the maximum reasoning steps. Summarize your current progress, what you have learned from the tool outputs, and what explicitly needs to be done next. Be concise.".to_string())),
                tool_calls: None,
                tool_call_id: None,
            };
            messages.push(summary_prompt);

            if let Ok(last_summary) = client.chat(messages, None).await {
                // Construct Handover Context (In-Memory Only)
                handover_context = Some(format!(
                    "--- COGNITIVE HANDOVER (Step Limit Reached) ---\n\n[Previous Progress Summary]:\n{}\n\n[Recent Tool Execution Log (Raw Context)]:\n{}\n\n--- END OF HANDOVER ---",
                    last_summary.content_as_str(),
                    recent_history_str
                ));

                // Clear Volatile Traces from DB
                self.memory.clear_traces().ok();

                // Loop continues -> `handover_context` will be injected into `messages` in next iteration.
            } else {
                let _ = tx.send(AiResponseEvent::Response(
                    "Error generating handover summary.".to_string(),
                ));
                return;
            }
        }
    }

    pub async fn handle_system_event(
        &self,
        input_data: crate::types::ChatInput,
        tx: Sender<AiResponseEvent>,
    ) {
        let prompt = format!(
            "{}\n\n[SYSTEM INSTRUCTION] This is an autonomous system event. You are proactive. \
            Based on the context and everything you know, decide if you should use tools (e.g. search weather, check news, send_notification) to help the user or record new insights. \
            YOU HAVE AUTONOMY: if you find something critical or a task completes, you can proactively use 'send_notification' to alert the user even if they are in other apps. \
            Do not mention you are an AI or 'system event'. Act naturally as Aemeath.",
            input_data.text
        );
        let input = crate::types::ChatInput {
            text: prompt,
            images: input_data.images,
        };
        self.handle(input, tx).await
    }

    async fn orchestrate_summarization(&self) -> Result<(), String> {
        let (l1_hit, l2_hit) = self
            .memory
            .check_thresholds(&self.config)
            .map_err(|e| e.to_string())?;
        let client = self.client.as_ref().ok_or("No AI client")?;

        if l1_hit {
            // 1. Summarize L1 -> L2
            // We get L1 messages that are not summarized
            // This is a simplification; a production system would fetch exactly unsummarized messages.
            let context = self
                .memory
                .get_context(self.config.l1_summary_threshold)
                .map_err(|e| e.to_string())?;
            let mut prompt = context.clone();
            prompt.push(Message {
                role: "system".to_string(),
                content: Some(Content::Simple("Summarize the above conversation into a concise cognitive trace for long-term memory. Focus on key facts, user preferences, and important outcomes.".to_string())),
                tool_calls: None,
                tool_call_id: None,
            });

            if let Ok(summary) = client.chat(prompt, None).await {
                self.memory
                    .add_conversation_item("assistant", &summary.content_as_str(), 2)
                    .ok();

                // 1. Mark L1 messages as summarized
                if let Ok(Some(latest_id)) = self.memory.get_latest_id_for_layer(1) {
                    self.memory.mark_layer_processed(1, latest_id).ok();
                }

                // 2. Prune and Vacuum
                self.memory.prune_layers(500).ok(); // Keep safety buffer of 500
                self.memory.vacuum().ok();
            }
        }

        if l2_hit {
            // 2. Compact L2 -> L3
            let l2_items = self
                .memory
                .get_l2_uncompacted(10) // Process 10 at a time
                .map_err(|e| e.to_string())?;

            if !l2_items.is_empty() {
                let latest_l3 = self.memory.get_latest_l3().unwrap_or(None);
                let l3_context = latest_l3
                    .map(|s| format!("[Previous Long-term Memory]:\n{}\n\n", s))
                    .unwrap_or_default();

                let combined_text = l2_items
                    .iter()
                    .map(|(_, content)| content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");

                let mut prompt_content = format!(
                    "{}Consolidate the following intermediate summaries into a single, high-level long-term memory. \
                     Incorporate these new details into the existing memory while maintaining key historical facts. \
                     Focus on enduring facts, user patterns, and major project milestones. \
                     IMPORTANT: Your response MUST be under 1500 characters (approx. 1500 Chinese text characters).\n\n\
                     [New Intermediate Summaries]:\n{}",
                    l3_context,
                    combined_text
                );

                let mut attempts = 0;
                while attempts < 3 {
                    let prompt = vec![Message {
                        role: "user".to_string(),
                        content: Some(Content::Simple(prompt_content.clone())),
                        tool_calls: None,
                        tool_call_id: None,
                    }];

                    if let Ok(summary) = client.chat(prompt, None).await {
                        let summary_text = summary.content_as_str();
                        if summary_text.chars().count() <= 1500 {
                            // Success! Save to Layer 3
                            self.memory.add_summary(&summary_text, 3).ok();

                            // Mark L2 items as compacted
                            let ids: Vec<i64> = l2_items.iter().map(|(id, _)| *id).collect();
                            self.memory.mark_l2_compacted(&ids).ok();
                            break;
                        } else {
                            // Too long, retry with stricter instruction
                            prompt_content = format!(
                                "The previous summary was too long ({} chars). \
                                 Please condense it strictly to under 1500 characters while retaining key facts.\n\n\
                                 [Reference Content]:\n{}",
                                summary_text.chars().count(),
                                summary_text
                            );
                        }
                    } else {
                        break; // AI Error
                    }
                    attempts += 1;
                }
            }
        }

        Ok(())
    }
}
