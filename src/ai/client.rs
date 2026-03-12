use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: Content,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Content {
    Simple(String),
    Multimodal(Vec<ContentPart>),
}

impl Default for Content {
    fn default() -> Self {
        Content::Simple(String::new())
    }
}

impl Content {
    pub fn as_str(&self) -> &str {
        match self {
            Content::Simple(s) => s,
            Content::Multimodal(parts) => {
                for part in parts {
                    if let ContentPart::Text { text } = part {
                        return text;
                    }
                }
                ""
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone)]
pub struct OpenAiClient {
    api_key: String,
    base_url: String,
    model: String,
    http_client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Choice {
    pub message: Message,
}

impl OpenAiClient {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self {
            api_key: api_key.trim().to_string(),
            base_url: base_url.trim().to_string(),
            model: model.trim().to_string(),
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Value>>,
    ) -> Result<Message, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        
        // Detailed request context
        let role_counts = messages.iter().fold(std::collections::HashMap::new(), |mut acc, m| {
            *acc.entry(&m.role).or_insert(0) += 1;
            acc
        });
        tracing::info!("AI Request | URL: {} | Model: {} | Messages: {} ({:?})", 
            url, self.model, messages.len(), role_counts
        );

        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            tools,
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Request Transport Error: {}", e);
                format!("Request failed: {}", e)
            })?;

        let status = response.status();
        tracing::info!("AI Response Status: {}", status);

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            tracing::error!("API Error Body: {}", error_text);
            return Err(format!("API Error ({}): {}", status, error_text));
        }

        let body_text = response.text().await.map_err(|e| {
            tracing::error!("Failed to get response text: {}", e);
            format!("Failed to get body: {}", e)
        })?;

        let chat_response: ChatResponse = serde_json::from_str(&body_text).map_err(|e| {
            let preview = if body_text.chars().count() > 500 {
                format!("{}...", body_text.chars().take(500).collect::<String>())
            } else {
                body_text.clone()
            };
            tracing::error!("JSON Parse Error: {} | Body Content: {}", e, preview);
            format!("Failed to parse response: {}", e)
        })?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.clone())
            .ok_or_else(|| "No response from AI".to_string())
    }
}
