use crate::ai::client::{Message, OpenAiClient};
use crate::ai::memory::MemoryManager;
use crate::ai::skills::SkillManager;
use crate::types::AiConfig;
use std::sync::Arc;

pub struct ChatKernel {
    config: AiConfig,
    client: Option<OpenAiClient>,
    memory: Arc<MemoryManager>,
    skills: SkillManager,
}

impl ChatKernel {
    pub fn new(config: &AiConfig, scheduler: crate::interaction::ActionScheduler) -> Self {
        let client = if config.api_key.is_empty() {
            None
        } else {
            Some(OpenAiClient::new(config.clone()))
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

    pub async fn handle(&self, input: String) -> String {
        let client = match &self.client {
            Some(c) => c,
            None => return "Please configure your AI settings first!".to_string(),
        };

        // 1. Initial User Message
        // Clear volatile tool traces from previous sessions/requests
        self.memory.clear_traces().ok();

        let (db_content, llm_content) = if let Some(idx) = input.find("\n\n[SYSTEM INSTRUCTION]") {
            (input[..idx].to_string(), input.clone())
        } else {
            (input.clone(), input.clone())
        };

        let user_msg = Message {
            role: "user".to_string(),
            content: db_content,
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
                        content: self.config.system_prompt.clone(),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                );
            }

            // Inject in-memory handover context if available
            if let Some(ctx) = &handover_context {
                messages.push(Message {
                    role: "system".to_string(),
                    content: ctx.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }

            // INJECT CURRENT USER MESSAGE (Deferred Persistence Fix)
            messages.push(Message {
                role: "user".to_string(),
                content: llm_content.clone(),
                tool_calls: None,
                tool_call_id: None,
            });
            println!(
                "[Kernel] Context prepared. Message count: {}",
                messages.len()
            );

            let tools = self.skills.get_tools_for_llm();
            let tools_opt = if tools.is_empty() {
                println!("[Kernel] No tools available.");
                None
            } else {
                println!("[Kernel] Tools available: {}", tools.len());
                Some(tools)
            };

            let mut turns = 0;
            let max_turns = self.config.react_limit;

            let mut final_response = None;

            while turns < max_turns {
                turns += 1;
                println!("[Kernel] Turn {}/{}", turns, max_turns);

                match client.chat(messages.clone(), tools_opt.clone()).await {
                    Ok(response_msg) => {
                        println!("[Kernel] LLM Response Role: {}", response_msg.role);
                        if let Some(calls) = &response_msg.tool_calls {
                            println!("[Kernel] Tool Calls detected: {}", calls.len());
                        }

                        messages.push(response_msg.clone());

                        if let Some(tool_calls) = &response_msg.tool_calls {
                            // Execution Phase
                            for tool_call in tool_calls {
                                let skill_name = &tool_call.function.name;
                                let args_str = &tool_call.function.arguments;
                                println!(
                                    "[Kernel] Executing Tool: {} with args: {}",
                                    skill_name, args_str
                                );

                                let args_result: Result<serde_json::Value, _> =
                                    serde_json::from_str(args_str);
                                let args = args_result.unwrap_or_else(|e| {
                                    println!("[Kernel] JSON Parse Error: {}", e);
                                    serde_json::json!({})
                                });

                                let result = if let Some(skill) = self.skills.get(skill_name) {
                                    match skill.execute(args).await {
                                        Ok(out) => {
                                            println!("[Kernel] Tool Output: {:.100}...", out); // Truncate log
                                            out
                                        }
                                        Err(err) => {
                                            let e = format!("Error: {}", err);
                                            println!("[Kernel] Tool Execution Failed: {}", e);
                                            e
                                        }
                                    }
                                } else {
                                    let e = format!("Unknown tool: {}", skill_name);
                                    println!("[Kernel] {}", e);
                                    e
                                };

                                let tool_response = Message {
                                    role: "tool".to_string(),
                                    content: result,
                                    tool_calls: None,
                                    tool_call_id: Some(tool_call.id.clone()),
                                };

                                // Store in Layer 2 (Traces)
                                self.memory.add_trace(&response_msg).ok();
                                self.memory.add_trace(&tool_response).ok();

                                messages.push(tool_response);
                            }
                        } else {
                            println!("[Kernel] No tool calls. Final response received.");
                            // Completion Phase: Store final response in Layer 1
                            self.memory.add_message(&user_msg).ok(); // Save initiated user message now
                            self.memory.add_message(&response_msg).ok(); // This is the final answer
                            final_response = Some(response_msg.content);
                            break; // Break inner loop
                        }
                    }
                    Err(e) => {
                        println!("[Kernel] AI Client Error: {}", e);
                        return format!("AI Error: {}", e);
                    }
                }
            }

            if let Some(content) = final_response {
                // Orchestrate summarization (L1 -> L2) if needed
                let kernel_clone = Arc::new(Self {
                    config: self.config.clone(),
                    client: self.client.clone(),
                    memory: Arc::clone(&self.memory),
                    skills: self.skills.clone(),
                });
                tokio::spawn(async move {
                    kernel_clone.orchestrate_summarization().await.ok();
                });
                return content;
            }

            // If we are here, we hit max_turns without a final response.
            // Cognitive Handover
            total_handovers += 1;
            if total_handovers >= MAX_HANDOVERS {
                return "Cognitive Limit Reached (Max Handovers). Terminating to prevent infinite loop.".to_string();
            }

            // Capture recent tool traces (last 5) from current messages BEFORE generating summary prompt
            let trace_capture_len = 5;
            let captured_traces = messages
                .iter()
                .rev()
                .filter(|m| m.role == "tool" || (m.role == "assistant" && m.tool_calls.is_some()))
                .take(trace_capture_len)
                .map(|m| {
                    let content_preview = if m.content.len() > 200 {
                        format!("{}...", &m.content[..200])
                    } else {
                        m.content.clone()
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
                content: "You have reached the maximum reasoning steps. Summarize your current progress, what you have learned from the tool outputs, and what explicitly needs to be done next. Be concise.".to_string(),
                tool_calls: None,
                tool_call_id: None,
            };
            messages.push(summary_prompt);

            if let Ok(last_summary) = client.chat(messages, None).await {
                // Construct Handover Context (In-Memory Only)
                handover_context = Some(format!(
                    "--- COGNITIVE HANDOVER (Step Limit Reached) ---\n\n[Previous Progress Summary]:\n{}\n\n[Recent Tool Execution Log (Raw Context)]:\n{}\n\n--- END OF HANDOVER ---",
                    last_summary.content,
                    recent_history_str
                ));

                // Clear Volatile Traces from DB
                self.memory.clear_traces().ok();

                // Loop continues -> `handover_context` will be injected into `messages` in next iteration.
            } else {
                return "Error generating handover summary.".to_string();
            }
        }
    }

    pub async fn handle_system_event(&self, event_context: String) -> String {
        let prompt = format!(
            "{}\n\n[SYSTEM INSTRUCTION] This is an autonomous system event. You are proactive. \
            Based on the context and everything you know, decide if you should use tools (e.g. search weather, check news, update_fact_board) to help the user or record new insights, \
            or just provide emotional value. If the context implies a need or a new fact about the user, use the tools immediately. \
            Do not mention you are an AI or 'system event'. Act naturally as Aemeath.",
            event_context
        );
        self.handle(prompt).await
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
                content: "Summarize the above conversation into a concise cognitive trace for long-term memory. Focus on key facts, user preferences, and important outcomes.".to_string(),
                tool_calls: None,
                tool_call_id: None,
            });

            if let Ok(summary) = client.chat(prompt, None).await {
                self.memory
                    .add_conversation_item("assistant", &summary.content, 2)
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
                        role: "system".to_string(),
                        content: prompt_content.clone(),
                        tool_calls: None,
                        tool_call_id: None,
                    }];

                    if let Ok(summary) = client.chat(prompt, None).await {
                        if summary.content.chars().count() <= 1500 {
                            // Success! Save to Layer 3
                            self.memory.add_summary(&summary.content, 3).ok();

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
                                summary.content.chars().count(),
                                summary.content
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
