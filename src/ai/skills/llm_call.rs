use crate::ai::client::{Content, ContentPart, ImageUrl, Message, OpenAiClient};
use crate::ai::skills::Skill;
use async_trait::async_trait;
use serde_json::{json, Value};

/// LlmCallSkill exposes the main agent's LLM client as a callable tool.
/// External skills, sub-agents, or the main agent itself can use this to
/// send prompts (with optional images) to the LLM without separate API config.
pub struct LlmCallSkill {
    client: OpenAiClient,
}

impl LlmCallSkill {
    pub fn new(client: OpenAiClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Skill for LlmCallSkill {
    fn name(&self) -> &str {
        "llm_call"
    }

    fn description(&self) -> &str {
        "Call the configured LLM directly with a custom prompt. \
         Supports text and optional image inputs (base64 or URL). \
         Use this when you or a sub-agent needs LLM capabilities like \
         image understanding, text analysis, translation, or summarization \
         without spawning a full sub-agent."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| "Missing 'prompt' parameter".to_string())?;

        let system = args["system"].as_str().unwrap_or("");

        tracing::info!(
            "[LlmCall] prompt={} chars, system={} chars",
            prompt.len(),
            system.len()
        );

        // Build messages
        let mut messages = Vec::new();

        // Optional system prompt
        if !system.is_empty() {
            messages.push(Message {
                role: "system".to_string(),
                content: Some(Content::Simple(system.to_string())),
                ..Default::default()
            });
        }

        // Build user message content (text + optional images)
        let images: Vec<&str> = args["images"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        if images.is_empty() {
            // Text-only
            messages.push(Message {
                role: "user".to_string(),
                content: Some(Content::Simple(prompt.to_string())),
                ..Default::default()
            });
        } else {
            // Multimodal (text + images)
            let mut parts = vec![ContentPart::Text {
                text: prompt.to_string(),
            }];

            for img in &images {
                let url = if img.starts_with("data:") || img.starts_with("http") {
                    img.to_string()
                } else {
                    // Assume base64, auto-detect mime type
                    format!("data:image/png;base64,{}", img)
                };
                parts.push(ContentPart::ImageUrl {
                    image_url: ImageUrl { url },
                });
            }

            tracing::info!("[LlmCall] Multimodal input: {} image(s)", images.len());

            messages.push(Message {
                role: "user".to_string(),
                content: Some(Content::Multimodal(parts)),
                ..Default::default()
            });
        }

        // Call LLM (no tools — pure text/vision inference)
        let response = self
            .client
            .chat(messages, None)
            .await
            .map_err(|e| format!("LLM call failed: {}", e))?;

        let result = response.content_as_str().to_string();

        let preview: String = result.chars().take(300).collect();
        tracing::info!("[LlmCall] Response {} chars: {}", result.len(), preview);

        if result.is_empty() {
            Ok("(LLM returned empty response)".to_string())
        } else {
            Ok(result)
        }
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
                        "prompt": {
                            "type": "string",
                            "description": "The prompt to send to the LLM"
                        },
                        "system": {
                            "type": "string",
                            "description": "Optional system prompt to set context/role"
                        },
                        "images": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional image inputs: base64 strings, data URIs, or HTTP URLs"
                        }
                    },
                    "required": ["prompt"]
                }
            }
        })
    }
}
