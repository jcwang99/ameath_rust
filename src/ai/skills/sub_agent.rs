use crate::ai::client::{Content, Message, OpenAiClient};
use crate::ai::skills::{Skill, SkillManager};
use async_trait::async_trait;
use serde_json::{json, Value};

/// SubAgentSkill allows the main agent to spawn independent sub-agents
/// that run in parallel, each with their own ReAct loop and tool access.
pub struct SubAgentSkill {
    client: OpenAiClient,
    manager: SkillManager,
}

impl SubAgentSkill {
    pub fn new(client: OpenAiClient, manager: SkillManager) -> Self {
        Self { client, manager }
    }

    /// Run a single sub-agent: mini ReAct loop with tool access.
    async fn run_one(
        client: &OpenAiClient,
        manager: &SkillManager,
        task: &str,
        tools_filter: &Option<Vec<String>>,
        react_limit: usize,
    ) -> Result<String, String> {
        // Build available tools
        let all_tools = manager.get_tools_for_llm();
        let tools: Vec<Value> = match tools_filter {
            Some(whitelist) if !whitelist.contains(&"*".to_string()) => {
                all_tools.into_iter().filter(|t| {
                    t.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .map_or(false, |name| whitelist.contains(&name.to_string()))
                }).collect()
            }
            _ => all_tools,
        };

        // Filter out the sub_agent tool itself to prevent recursive spawning
        let tools: Vec<Value> = tools.into_iter().filter(|t| {
            t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .map_or(true, |name| name != "sub_agent")
        }).collect();

        let tools_opt = if tools.is_empty() { None } else { Some(tools) };

        // Initial message: task as system prompt
        let mut messages = vec![
            Message {
                role: "system".to_string(),
                content: Some(Content::Simple(format!(
                    "You are a focused sub-agent. Complete the following task concisely.\n\
                     Task: {}\n\n\
                     Important: When done, output ONLY your final result. Be brief and factual.",
                    task
                ))),
                ..Default::default()
            },
            Message {
                role: "user".to_string(),
                content: Some(Content::Simple("Execute the task now.".to_string())),
                ..Default::default()
            },
        ];

        let mut turns = 0;
        let mut last_text = String::new();

        while turns < react_limit {
            turns += 1;
            tracing::debug!("[SubAgent] Turn {}/{} for task: {:.50}", turns, react_limit, task);

            let mut retries = 3;
            let response = loop {
                match client.chat(messages.clone(), tools_opt.clone()).await {
                    Ok(msg) => break msg,
                    Err(e) => {
                        retries -= 1;
                        if retries == 0 {
                            tracing::warn!("[SubAgent] LLM call failed after retries: {}", e);
                            return Err(format!("Sub-agent LLM error: {}", e));
                        }
                        tracing::warn!("[SubAgent] LLM call failed, retrying in 2s... ({})", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
            };

            // Capture text content
            let content_str = response.content_as_str().to_string();
            if !content_str.is_empty() {
                last_text = content_str;
            }

            // Check for tool calls
            let has_tool_calls = response.tool_calls.as_ref().map_or(false, |tc| !tc.is_empty());

            if !has_tool_calls {
                // No tool calls = sub-agent is done
                break;
            }

            // Process tool calls
            messages.push(response.clone());

            if let Some(tool_calls) = &response.tool_calls {
                for tc in tool_calls {
                    let skill_name = &tc.function.name;
                    let args: Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or_else(|_| json!({}));

                    tracing::info!("[SubAgent] Calling tool: {} args: {:.200}", skill_name, tc.function.arguments);

                    let result = if let Some(skill) = manager.get(skill_name) {
                        match skill.execute(args).await {
                            Ok(out) => {
                                let preview: String = out.chars().take(200).collect();
                                tracing::info!("[SubAgent] Tool '{}' returned {} chars: {}", skill_name, out.len(), preview);
                                out
                            }
                            Err(e) => {
                                tracing::warn!("[SubAgent] Tool '{}' error: {}", skill_name, e);
                                format!("Tool error: {}", e)
                            }
                        }
                    } else {
                        tracing::warn!("[SubAgent] Unknown tool: {}", skill_name);
                        format!("Unknown tool: {}", skill_name)
                    };

                    messages.push(Message {
                        role: "tool".to_string(),
                        content: Some(Content::Simple(result)),
                        tool_call_id: Some(tc.id.clone()),
                        ..Default::default()
                    });
                }
            }
        }

        if last_text.is_empty() {
            tracing::info!("[SubAgent] Completed in {} turns (no output) | task: {:.80}", turns, task);
            Ok("(sub-agent produced no output)".to_string())
        } else {
            let preview: String = last_text.chars().take(300).collect();
            tracing::info!(
                "[SubAgent] Completed in {} turns ({} chars) | task: {:.80} | result: {}",
                turns, last_text.len(), task, preview
            );
            Ok(last_text)
        }
    }
}

#[async_trait]
impl Skill for SubAgentSkill {
    fn name(&self) -> &str {
        "sub_agent"
    }

    fn description(&self) -> &str {
        "Spawn one or more independent sub-agents to process tasks in parallel. \
         Each sub-agent has its own context and can use tools. \
         Use this when a task can be decomposed into independent sub-tasks \
         that would exceed the current context window."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let tasks: Vec<String> = if let Some(arr) = args["tasks"].as_array() {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        } else if let Some(s) = args["task"].as_str() {
            vec![s.to_string()]
        } else {
            return Err("Missing 'tasks' (array) or 'task' (string) parameter".to_string());
        };

        if tasks.is_empty() {
            return Err("No tasks provided".to_string());
        }

        let tools_filter: Option<Vec<String>> = args["tools"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });

        let react_limit = args["react_limit"].as_u64().unwrap_or(10) as usize;

        tracing::info!("[SubAgent] Spawning {} sub-agent(s), react_limit={}", tasks.len(), react_limit);

        // Spawn all sub-agents in parallel
        let mut handles = Vec::new();
        for (i, task) in tasks.iter().enumerate() {
            let client = self.client.clone();
            let manager = self.manager.clone();
            let task = task.clone();
            let tools_filter = tools_filter.clone();

            handles.push(tokio::spawn(async move {
                let result = Self::run_one(&client, &manager, &task, &tools_filter, react_limit).await;
                (i, task, result)
            }));
        }

        // Collect results
        let mut results: Vec<(usize, String, Result<String, String>)> = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(r) => results.push(r),
                Err(e) => {
                    tracing::warn!("[SubAgent] Task panicked: {}", e);
                }
            }
        }
        results.sort_by_key(|(i, _, _)| *i);

        // Format output
        let total = results.len();
        let success_count = results.iter().filter(|(_, _, r)| r.is_ok()).count();
        let mut output = format!("[Sub-Agent Results: {}/{} succeeded]\n\n", success_count, total);

        for (i, task, result) in &results {
            let task_preview: String = task.chars().take(80).collect();
            match result {
                Ok(text) => {
                    output.push_str(&format!("--- Agent #{} ({}) ---\n{}\n\n", i + 1, task_preview, text));
                }
                Err(e) => {
                    output.push_str(&format!("--- Agent #{} ({}) [FAILED] ---\n{}\n\n", i + 1, task_preview, e));
                }
            }
        }

        tracing::info!("[SubAgent] All done: {}/{} succeeded, total output {} chars", success_count, total, output.len());

        Ok(output)
    }

    fn to_tool(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tasks": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of task descriptions. Each becomes an independent sub-agent running in parallel."
                        },
                        "tools": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tool whitelist for sub-agents. Use [\"*\"] for all tools. Omit to allow all."
                        },
                        "react_limit": {
                            "type": "integer",
                            "description": "Max ReAct turns per sub-agent (default 10)"
                        }
                    },
                    "required": ["tasks"]
                }
            }
        })
    }
}
