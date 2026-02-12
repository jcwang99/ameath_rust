use crate::ai::client::{Message, OpenAiClient};
use crate::types::AiConfig;

pub struct ChatKernel {
    client: Option<OpenAiClient>,
}

impl ChatKernel {
    pub fn new(config: &AiConfig) -> Self {
        if config.api_key.is_empty() {
            Self { client: None }
        } else {
            Self {
                client: Some(OpenAiClient::new(config.clone())),
            }
        }
    }

    pub async fn handle(&self, input: String) -> String {
        if let Some(client) = &self.client {
            let messages = vec![
                Message {
                    role: "system".to_string(),
                    content: "You are Ameath, a helpful pet assistant inside a desktop application. Keep your responses short and friendly.".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: input,
                },
            ];

            match client.chat(messages).await {
                Ok(reply) => reply,
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("{") && err_str.contains("error") {
                        // Attempt to extract friendly message from JSON
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&err_str) {
                            if let Some(msg) = v["error"]["message"].as_str() {
                                return format!("AI Error: {}", msg);
                            }
                        }
                    }
                    format!("Error: {}", e)
                }
            }
        } else {
            "Please configure your AI settings first!".to_string()
        }
    }
}
