use crate::ai::client::{Content, ContentPart, ImageUrl, Message, OpenAiClient};
use crate::ai::memory::MemoryManager;
use crate::ai::skills::SkillManager;
use crate::types::{AiConfig, AiResponseEvent, ThinkingState};
use std::sync::mpsc::Sender;
use std::sync::Arc;

fn next_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub struct ChatKernel {
    config: AiConfig,
    client: Option<OpenAiClient>,
    memory: Arc<MemoryManager>,
    skills: SkillManager,
}

impl ChatKernel {
    pub fn new(config: &AiConfig, scheduler: crate::interaction::ActionScheduler, shared_routines: std::sync::Arc<std::sync::Mutex<crate::types::RoutinesConfig>>) -> Self {
        let profile = config.active_profile();
        let client = if profile.api_key.is_empty() {
            None
        } else {
            Some(OpenAiClient::new(
                profile.api_key.clone(),
                profile.base_url.clone(),
                profile.model.clone(),
                profile.use_responses_api,
            ))
        };

        let memory = Arc::new(MemoryManager::new());
        let skills = SkillManager::new(Arc::clone(&memory), config, scheduler, shared_routines);

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
        let request_id = next_request_id();

        let mut log_msg = if input.chars().count() > 50 {
            format!("[INPUT_TRACE] Received: {}...", input.chars().take(47).collect::<String>().replace("\n", " "))
        } else {
            format!("[INPUT_TRACE] Received: {}", input.trim().replace("\n", " "))
        };
        if !images.is_empty() {
            log_msg.push_str(&format!(" (Images: {})", images.len()));
        }
        tracing::info!("{}", log_msg);
        let client = match &self.client {
            Some(c) => c,
            None => {
                let _ = tx.send(AiResponseEvent::Response(
                    "Please configure your AI settings first!".to_string(),
                ));
                return;
            }
        };

        // 1. Initial User Message
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

        // --- #todo FAST-TRACK ---
        if input.trim_start().starts_with("#todo ") {
            let todo_content = input.trim_start()["#todo ".len()..].trim();
            if !todo_content.is_empty() {
                match crate::ai::skills::todo_skill::TodoSkill::add_todo_local(todo_content) {
                    Ok(_) => {
                        let confirmation = "收到！已经加入待办清单啦~ [IMG]assets/stickers/OK.gif";
                        
                        self.memory.add_conversation_item("user", &input, 1).ok();
                        self.memory.add_conversation_item("assistant", confirmation, 1).ok();
                        
                        let _ = tx.send(AiResponseEvent::Response(confirmation.to_string()));
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(AiResponseEvent::Response(format!("待办添加失败: {}", e)));
                        return;
                    }
                }
            }
        }
        // --- #memo FAST-TRACK ---
        if input.trim_start().starts_with("#memo ") {
            let memo_content = input.trim_start()["#memo ".len()..].trim();
            if !memo_content.is_empty() {
                match crate::ai::skills::memo_skill::MemoSkill::add_memo_local(memo_content) {
                    Ok(_) => {
                        let confirmation = "记下来啦！我会帮你盯着的~ [IMG]assets/stickers/好的.gif";
                        
                        self.memory.add_conversation_item("user", &input, 1).ok();
                        self.memory.add_conversation_item("assistant", confirmation, 1).ok();
                        
                        let _ = tx.send(AiResponseEvent::Response(confirmation.to_string()));
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(AiResponseEvent::Response(format!("备忘添加失败: {}", e)));
                        return;
                    }
                }
            }
        }
        // -----------------------

        let is_system_event = input.find("\n\n[SYSTEM INSTRUCTION]").is_some();
        let (db_content, llm_content) = if let Some(idx) = input.find("\n\n[SYSTEM INSTRUCTION]") {
            (input[..idx].to_string(), input.clone())
        } else {
            (input.clone(), input.clone())
        };

        let mut parts = vec![ContentPart::Text { text: llm_content }];

        for img in &images {
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
            ..Default::default()
        };
        // DEFER SAVING until we have a response
        // self.memory.add_message(&user_msg).ok();

        // 2. ReAct Loop (Infinite with Handover)
        let mut total_handovers = 0;
        const MAX_HANDOVERS: usize = 3;
        let mut handover_context: Option<String> = None;

        loop {
            // Refresh context from memory (picks up new summaries and cleared traces)
            let is_multimodal = self.config.active_profile().is_multimodal;
            let mut messages = self
                .memory
                .get_context_for_request(self.config.l1_summary_threshold, is_multimodal, Some(&request_id))
                .unwrap_or_default();

            // Inject Base System Prompt (Configurable Persona)
            if !self.config.system_prompt.is_empty() {
                let current_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                let prompt_to_use = format!("{}\n\n[Current System Time]: {}", self.config.system_prompt, current_time);
                messages.insert(
                    0,
                    Message {
                        role: "system".to_string(),
                        content: Some(Content::Simple(prompt_to_use)),
                        ..Default::default()
                    },
                );
            }

            // Inject in-memory handover context if available
            if let Some(ctx) = &handover_context {
                messages.push(Message {
                    role: "system".to_string(),
                    content: Some(Content::Simple(ctx.clone())),
                    ..Default::default()
                });
            }

            // --- Inject Dynamic Todo Context ---
            let pending_todos: Vec<_> = crate::ai::skills::todo_skill::TodoSkill::list_todos_local(true);
            
            if !pending_todos.is_empty() {
                let mut todo_list_str = String::from("[Current Pending Todos]\n");
                for (i, t) in pending_todos.iter().enumerate() {
                    todo_list_str.push_str(&format!("{}. ({}) {}\n", i + 1, &t.id[..4], t.content));
                }
                
                let todo_instruction = "\n[Instruction]\n\
                    These are the user's current pending tasks. Please act as a proactive assistant: \
                    be aware of these tasks, and naturally remind or suggest follow-up actions to the user \
                    during the conversation whenever relevant, helping them stay productive.";
                
                messages.push(Message {
                    role: "system".to_string(),
                    content: Some(Content::Simple(format!("{}{}", todo_list_str, todo_instruction))),
                    ..Default::default()
                });
            }
            // -----------------------------------

            // --- Inject Dynamic Memo Context ---
            let memos = crate::ai::skills::memo_skill::MemoSkill::list_memos_local();
            if !memos.is_empty() {
                let mut memo_str = String::from("[User Memos (Things to remember)]\n");
                for (i, m) in memos.iter().enumerate() {
                    memo_str.push_str(&format!("{}. {}\n", i + 1, m.content));
                }
                
                let memo_instruction = "\n[Instruction]\n\
                    These are things the user wants to remember. If the user seems to have forgotten something \
                    mentioned here, or if the context is highly relevant, please proactively remind them in a natural way. \
                    IMPORTANT: Once the user confirms a memo event is completed or no longer needed, you SHOULD proactively \
                    call 'delete_memo' to keep the list clean and focused.";
                
                messages.push(Message {
                    role: "system".to_string(),
                    content: Some(Content::Simple(format!("{}{}", memo_str, memo_instruction))),
                    ..Default::default()
                });
            }
            // -----------------------------------

            // INJECT CURRENT USER MESSAGE (Deferred Persistence Fix)
            messages.push(user_msg.clone());
            tracing::debug!("Context prepared. Message count: {}", messages.len());

            // --- ANTI-HALLUCINATION INTENT DECLARATION (inject once) ---
            let initial_tools = self.skills.get_tools_for_llm();
            if !initial_tools.is_empty() {
                messages.push(Message {
                    role: "user".to_string(),
                    content: Some(Content::Simple(
                        "\n\n[SYSTEM REQUIREMENT]\nYou MUST conclude your response by declaring your tool usage intent. \
                        Append exactly one of these tags at the VERY END of your text response: \
                        If you will NOT use a tool: '[TOOL_INTENT: NO]' \
                        If you WILL use a tool: '[TOOL_INTENT: YES]' \
                        Do not forget this tag!".to_string()
                    )),
                    ..Default::default()
                });
            }

            let mut turns = 0;
            let max_turns = self.config.react_limit;
            let profile = self.config.active_profile();
            tracing::info!(
                "Starting ReAct loop with Profile: {} | Context: {} msgs | Limit: {} turns",
                profile.name,
                messages.len(),
                max_turns
            );

            let mut fc_traces = Vec::new();
            let mut content_accumulator = Vec::new();

            while turns < max_turns {
                turns += 1;

                // Refresh tools each turn (supports dynamically loaded external skills)
                let tools = self.skills.get_tools_for_llm();
                let tools_opt = if tools.is_empty() {
                    None
                } else {
                    Some(tools)
                };
                tracing::info!("Turn {}/{}", turns, max_turns);
                let _ = tx.send(AiResponseEvent::Status(ThinkingState::Network));
                
                tracing::debug!("Requesting LLM with {} messages", messages.len());
                let mut retries = 0;
                let max_retries = 3;
                let mut response_result = client.chat(messages.clone(), tools_opt.clone()).await;
                
                while response_result.is_err() && retries < max_retries {
                    retries += 1;
                    tracing::warn!("AI request failed, retrying {}/{}...", retries, max_retries);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    response_result = client.chat(messages.clone(), tools_opt.clone()).await;
                }
                let _ = tx.send(AiResponseEvent::Status(ThinkingState::Standard));

                match response_result {
                    Ok(response_msg) => {
                        let original_content_str = response_msg.content_as_str();
                        
                        // Parse Tool Intent
                        let mut stripped_content = original_content_str.to_string();
                        let mut is_declaring_tool = false;
                        let mut tag_found = false;
                        
                        // Re-parse response to extract and strip the [TOOL_INTENT: ...] tag
                        if let Some(idx) = stripped_content.rfind("[TOOL_INTENT:") {
                            let sub = &stripped_content[idx..];
                            if let Some(end_idx) = sub.find(']') {
                                tag_found = true;
                                let intent_str = sub["[TOOL_INTENT:".len()..end_idx].trim().to_uppercase();
                                if intent_str.contains("YES") || intent_str.contains("TRUE") {
                                    is_declaring_tool = true;
                                } else if !intent_str.contains("NO") && !intent_str.contains("NONE") && !intent_str.contains("FALSE") {
                                    // If it's a spelling mistake like [TOOL_INTENT: search_web], we still assume it intends to use a tool
                                    is_declaring_tool = true;
                                }
                                
                                stripped_content.replace_range(idx..(idx + end_idx + 1), "");
                                stripped_content = stripped_content.trim_end().to_string();
                            }
                        }

                        let has_actual_tool_calls = response_msg.tool_calls.as_ref().map_or(false, |tc| !tc.is_empty());

                        // Verification Logic
                        // Case 1 (HALLUCINATION): Declared YES but no tool_calls emitted.
                        //   This is always dangerous regardless of turn number — intercept and force retry.
                        // Case 2 (TENSE-CONFUSION): Declared NO but tool_calls were emitted.
                        //   This is a minor mis-declaration, commonly caused by the model mixing up
                        //   "I used a tool earlier" vs "I'm using a tool now". Only intercept on Turn 1
                        //   to avoid disrupting mid-loop summaries after tool results come back.
                        let hallucination = tag_found && is_declaring_tool && !has_actual_tool_calls;
                        let tense_confusion = tag_found && !is_declaring_tool && has_actual_tool_calls;

                        // Use stripped content downstream
                        let content_str = stripped_content.as_str();

                        let content_preview = {
                            let char_count = content_str.chars().count();
                            if char_count > 100 {
                                let start: String = content_str.chars().take(50).collect();
                                let end: String = content_str.chars().skip(char_count - 50).collect();
                                format!("{} ... {}", start, end)
                            } else {
                                content_str.to_string()
                            }
                        };

                        tracing::info!("LLM Response Received | Role: {} | Content: \"{}\" | Intent Check: Tag={}, DeclaringTool={}",
                            response_msg.role,
                            content_preview.replace("\n", " "),
                            tag_found, is_declaring_tool
                        );

                        // 1. Emit response immediately if content exists (even if it's a hallucination, so user sees it)
                        if !content_str.is_empty() {
                            content_accumulator.push(content_str.to_string());
                            let _ = tx.send(AiResponseEvent::Response(content_str.to_string()));
                        }

                        if hallucination {
                            tracing::warn!("LLM Hallucination on Turn {}: Declared [TOOL_INTENT: YES] but no tool_calls in response. Forcing retry.", turns);
                            messages.push(response_msg);
                            messages.push(Message {
                                role: "user".to_string(),
                                content: Some(Content::Simple(
                                    "SYSTEM ALERT: You declared intent to use a tool but failed to invoke the actual JSON `tool_calls`. You MUST emit the tool_calls JSON structure NOW. Do not just describe what you will do — actually do it.".to_string()
                                )),
                                ..Default::default()
                            });
                            continue;
                        } else if tense_confusion && turns == 1 {
                            tracing::warn!("LLM Tense-Confusion on Turn 1: Declared [TOOL_INTENT: NO] but tool_calls were emitted. Forcing retry.");
                            messages.push(response_msg);
                            messages.push(Message {
                                role: "user".to_string(),
                                content: Some(Content::Simple(
                                    "SYSTEM ALERT: You declared [TOOL_INTENT: NO] but you actually invoked a tool call. Intent declaration must match your actions!".to_string()
                                )),
                                ..Default::default()
                            });
                            continue;
                        } else if tense_confusion && turns > 1 {
                            tracing::debug!("Bypassed tense-confusion mismatch on Turn {} (likely past-tense summary).", turns);
                        }
                        if let Some(calls) = &response_msg.tool_calls {
                            if !calls.is_empty() {
                                tracing::info!("Tool Calls detected: {}", calls.len());
                            }
                        }

                        messages.push(response_msg.clone());

                        if let Some(tool_calls) = &response_msg.tool_calls {
                            if tool_calls.is_empty() {
                                tracing::info!("Tool calls field is present but empty. Breaking loop as final response.");
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
                                    tool_call_id: Some(tool_call.id.clone()),
                                    ..Default::default()
                                };

                                // Store in Layer 2 (Traces)
                                self.memory.add_trace_for_request(&request_id, &response_msg).ok();
                                self.memory.add_trace_for_request(&request_id, &tool_response).ok();

                                messages.push(tool_response);
                            }
                            let _ = tx.send(AiResponseEvent::Status(ThinkingState::Standard));
                        } else {
                            tracing::info!(
                                "No tool calls. Final response received. ({} turns)",
                                turns
                            );
                            
                            break; // Break inner loop
                        }
                    }
                    Err(e) => {
                        tracing::error!("AI Client Error: {}", e);
                        if is_system_event {
                            let _ = tx.send(AiResponseEvent::Response(format!("后台系统任务失败 (已重试3次): {}", e)));
                        } else {
                            let _ = tx.send(AiResponseEvent::Response(format!("AI Error: {}", e)));
                        }
                        let _ = tx.send(AiResponseEvent::Status(ThinkingState::None));
                        return;
                    }
                }
            }

            if !content_accumulator.is_empty() || !fc_traces.is_empty() {
                let combined_original_content = content_accumulator.join("\n\n");
                
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
                            ..Default::default()
                        }];
                        if let Ok(summary_msg) = client.chat(summary_prompt, None).await {
                            combined_traces = summary_msg.content_as_str().to_string();
                        }
                    }
                    Some(combined_traces)
                } else {
                    None
                };

                // 2. Prepare content for DB (with FC traces)
                let db_content = if let Some(traces) = fc_summary {
                    if combined_original_content.is_empty() {
                        format!("--- FC 调用过程记录 ---\n{}", traces)
                    } else {
                        format!("{}\n\n--- FC 调用过程记录 ---\n{}", combined_original_content, traces)
                    }
                } else {
                    combined_original_content.clone()
                };

                // 3. Save to Memory (L1)
                let user_msg_id = self.memory.add_message_returning_id(&user_msg).unwrap_or(0);
                
                // If it's a multimodal request with images, spawn async task to generate description
                if user_msg_id > 0 && !images.is_empty() && self.config.active_profile().is_multimodal {
                    let client_clone = client.clone();
                    let memory_clone = Arc::clone(&self.memory);
                    let images_clone = images.clone();
                    
                    tokio::spawn(async move {
                        let mut prompt_parts = vec![ContentPart::Text { 
                            text: "请用简短客观的语言描述这张/这些图片的内容，重点提取画面中的关键信息（如人物特征、场景、核心物件、明显意图），以便后续脱离图片阅读这段文字也能理解上下文。不要包含多余寒暄。".to_string() 
                        }];
                        for img in images_clone {
                            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &img.data);
                            prompt_parts.push(ContentPart::ImageUrl {
                                image_url: ImageUrl {
                                    url: format!("data:{};base64,{}", img.mime_type, b64),
                                },
                            });
                        }
                        
                        let prompt_msg = Message {
                            role: "user".to_string(),
                            content: Some(Content::Multimodal(prompt_parts)),
                            ..Default::default()
                        };
                        
                        let mut attempts = 0;
                        let mut desc_result = String::new();
                        while attempts < 3 {
                            if let Ok(resp) = client_clone.chat(vec![prompt_msg.clone()], None).await {
                                let txt = resp.content_as_str().trim();
                                if !txt.is_empty() {
                                    desc_result = txt.to_string();
                                    break;
                                }
                            }
                            attempts += 1;
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        }
                        
                        if desc_result.is_empty() {
                            desc_result = "[未能成功生成此图片的总结代述]".to_string();
                        }
                        
                        let _ = memory_clone.update_image_desc(user_msg_id, &desc_result);
                    });
                }
                
                let response_msg = Message {
                    role: "assistant".to_string(),
                    content: Some(Content::Simple(db_content)),
                    ..Default::default()
                };
                self.memory.add_message(&response_msg).ok();

                // 4. Send to UI (Already done incrementally)

                // Clear traces AFTER the turn is fully complete and saved to Layer 1
                self.memory.clear_traces_for_request(&request_id).ok();
                
                // Prune expired images to save DB space (keep last 5)
                self.memory.prune_expired_images(5).ok();

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
                let _ = tx.send(AiResponseEvent::Status(ThinkingState::None));
                return;
            }

            // If we are here, we hit max_turns without a final response.
            // Cognitive Handover
            total_handovers += 1;
            if total_handovers >= MAX_HANDOVERS {
                let _ = tx.send(AiResponseEvent::Response("Cognitive Limit Reached (Max Handovers). Terminating to prevent infinite loop.".to_string()));
                let _ = tx.send(AiResponseEvent::Status(ThinkingState::None));
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
                ..Default::default()
            };
            messages.push(summary_prompt);

            if let Ok(last_summary) = client.chat(messages, None).await {
                let summary_text = last_summary.content_as_str().to_string();
                
                // Construct UI intermediate for handover
                let ui_handover_msg = format!("(思考中) 刚才我：\n{}\n\n正在继续处理...", summary_text);
                content_accumulator.push(ui_handover_msg.clone());
                let _ = tx.send(AiResponseEvent::Response(ui_handover_msg));

                // Construct Handover Context (In-Memory Only)
                handover_context = Some(format!(
                    "--- COGNITIVE HANDOVER (Step Limit Reached) ---\n\n[Previous Progress Summary]:\n{}\n\n[Recent Tool Execution Log (Raw Context)]:\n{}\n\n--- END OF HANDOVER ---",
                    summary_text,
                    recent_history_str
                ));

                // Clear Volatile Traces from DB
                self.memory.clear_traces_for_request(&request_id).ok();

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
        tracing::info!("[Kernel] handle_system_event | text: {:.100}, images: {}", input_data.text, input_data.images.len());
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
        let _guard = match self.memory.try_start_summarization() {
            Some(guard) => guard,
            None => {
                tracing::debug!("[Kernel] Summarization skipped: another task is already running");
                return Ok(());
            }
        };

        let (l1_hit, l2_hit) = self
            .memory
            .check_thresholds(&self.config)
            .map_err(|e| e.to_string())?;
        let client = self.client.as_ref().ok_or("No AI client")?;

        if l1_hit {
            // 1. Summarize L1 -> L2
            let l1_batch = self
                .memory
                .get_l1_unsummarized_batch(self.config.l1_summary_threshold)
                .map_err(|e| e.to_string())?;
            let context: Vec<Message> = l1_batch.iter().map(|(_, msg)| msg.clone()).collect();
            let mut prompt = context.clone();
            prompt.push(Message {
                role: "system".to_string(),
                content: Some(Content::Simple("Summarize the above conversation into a concise cognitive trace for long-term memory. Focus on key facts, user preferences, and important outcomes.".to_string())),
                ..Default::default()
            });

            if let Ok(summary) = client.chat(prompt, None).await {
                let summary_text = summary.content_as_str();
                tracing::info!("[Kernel] L1->L2 summary generated: {} chars", summary_text.len());
                self.memory
                    .add_conversation_item("assistant", summary_text, 2)
                    .ok();

                // 1. Mark L1 messages as summarized
                let processed_ids: Vec<i64> = l1_batch.iter().map(|(id, _)| *id).collect();
                if !processed_ids.is_empty() {
                    self.memory.mark_l1_summarized(&processed_ids).ok();
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
                     IMPORTANT: Your response MUST be under 3000 characters (approx. 3000 Chinese text characters).\n\n\
                     [New Intermediate Summaries]:\n{}",
                    l3_context,
                    combined_text
                );

                let mut attempts = 0;
                while attempts < 3 {
                    let prompt = vec![Message {
                        role: "user".to_string(),
                        content: Some(Content::Simple(prompt_content.clone())),
                        ..Default::default()
                    }];

                    if let Ok(summary) = client.chat(prompt, None).await {
                        let summary_text = summary.content_as_str();
                        if summary_text.chars().count() <= 3000 {
                            // Success! Save to Layer 3
                            tracing::info!("[Kernel] L2->L3 summary generated: {} chars, {} items compacted", summary_text.chars().count(), l2_items.len());
                            self.memory.add_summary(summary_text, 3).ok();

                            // Mark L2 items as compacted
                            let ids: Vec<i64> = l2_items.iter().map(|(id, _)| *id).collect();
                            self.memory.mark_l2_compacted(&ids).ok();
                            break;
                        } else {
                            // Too long, retry with stricter instruction
                            prompt_content = format!(
                                "The previous summary was too long ({} chars). \
                                 Please condense it strictly to under 3000 characters while retaining key facts.\n\n\
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
