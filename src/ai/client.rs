use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_reasoning: Option<String>,
}

impl Message {
    pub fn content_as_str(&self) -> &str {
        self.content.as_ref().map(|c| c.as_str()).unwrap_or("")
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            role: "user".to_string(),
            content: None,
            tool_calls: None,
            tool_call_id: None,
            reasoning_text: None,
            encrypted_reasoning: None,
        }
    }
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
    use_responses_api: bool,
    http_client: reqwest::Client,
}

// ===== Chat Completions API structures =====

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
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

// ===== Responses API structures =====

#[derive(Debug, Serialize)]
struct ResponsesRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    input: Vec<ResponsesInputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub(crate) enum ResponsesInputItem {
    #[serde(rename = "message")]
    Message {
        role: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<ResponsesInputContent>,
    },
    #[serde(rename = "reasoning")]
    Reasoning {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        encrypted_content: String,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        call_id: String,
        name: String,
        arguments: String,
    },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
}

/// Content in Responses API input can be a simple string or an array of content parts
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum ResponsesInputContent {
    Text(String),
    Parts(Vec<ResponsesInputContentPart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub(crate) enum ResponsesInputContentPart {
    #[serde(rename = "input_text")]
    InputText { text: String },
    #[serde(rename = "input_image")]
    InputImage { image_url: String },
}

// Responses API output types (parsed manually from serde_json::Value for robustness)
#[derive(Debug)]
pub(crate) enum ResponsesOutputItem {
    Message {
        content: Vec<ResponsesContentBlock>,
    },
    Reasoning {
        summary: String,
        encrypted_content: String,
    },
    FunctionCall {
        item_id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
}

#[derive(Debug)]
pub(crate) enum ResponsesContentBlock {
    OutputText { text: String },
}

/// Parse the Responses API JSON response into output items.
/// Uses Value-based extraction to tolerate unknown/extra fields gracefully.
pub(crate) fn parse_responses_api_output(body: &str) -> Result<Vec<ResponsesOutputItem>, String> {
    let root: Value = serde_json::from_str(body).map_err(|e| {
        format!("Failed to parse Responses API JSON: {}", e)
    })?;

    let output_arr = root.get("output").and_then(|v| v.as_array()).ok_or_else(|| {
        "Responses API: missing 'output' array in response".to_string()
    })?;

    let mut items = Vec::new();
    for item in output_arr {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match item_type {
            "message" => {
                let mut blocks = Vec::new();
                if let Some(content_arr) = item.get("content").and_then(|v| v.as_array()) {
                    for block in content_arr {
                        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if block_type == "output_text" {
                            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                                blocks.push(ResponsesContentBlock::OutputText {
                                    text: text.to_string(),
                                });
                            }
                        }
                        // Silently skip unknown content block types
                    }
                }
                items.push(ResponsesOutputItem::Message { content: blocks });
            }
            "reasoning" => {
                let summary_arr = item.get("summary").and_then(|v| v.as_array());
                let mut summary_text = String::new();
                if let Some(arr) = summary_arr {
                    for part in arr {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            summary_text.push_str(text);
                        }
                    }
                }
                let encrypted = item.get("encrypted_content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                items.push(ResponsesOutputItem::Reasoning {
                    summary: summary_text,
                    encrypted_content: encrypted,
                });
            }
            "function_call" => {
                let item_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let call_id = item.get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let arguments = item.get("arguments").and_then(|v| v.as_str()).unwrap_or("{}").to_string();
                items.push(ResponsesOutputItem::FunctionCall { item_id, call_id, name, arguments });
            }
            other => {
                tracing::debug!("Responses API: skipping unknown output item type: '{}'", other);
            }
        }
    }

    Ok(items)
}

// ===== Conversion helpers =====

/// Convert internal `Message` list to Responses API `input` items + top-level `instructions`.
pub(crate) fn convert_messages_to_responses_input(
    messages: &[Message],
) -> (Option<String>, Vec<ResponsesInputItem>) {
    let mut instructions: Option<String> = None;
    let mut items = Vec::new();

    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                // First system message becomes top-level `instructions`, rest become `system` role items
                if instructions.is_none() {
                    instructions = Some(msg.content_as_str().to_string());
                } else {
                    items.push(ResponsesInputItem::Message {
                        role: "system".to_string(),
                        content: Some(ResponsesInputContent::Text(
                            msg.content_as_str().to_string(),
                        )),
                    });
                }
            }
            "user" => {
                let content = match &msg.content {
                    Some(Content::Multimodal(parts)) => {
                        let resp_parts: Vec<ResponsesInputContentPart> = parts
                            .iter()
                            .map(|p| match p {
                                ContentPart::Text { text } => {
                                    ResponsesInputContentPart::InputText {
                                        text: text.clone(),
                                    }
                                }
                                ContentPart::ImageUrl { image_url } => {
                                    ResponsesInputContentPart::InputImage {
                                        image_url: image_url.url.clone(),
                                    }
                                }
                            })
                            .collect();
                        Some(ResponsesInputContent::Parts(resp_parts))
                    }
                    Some(Content::Simple(s)) => Some(ResponsesInputContent::Text(s.clone())),
                    None => None,
                };
                items.push(ResponsesInputItem::Message {
                    role: "user".to_string(),
                    content,
                });
            }
            "assistant" => {
                // If message has encrypted reasoning, push it first
                if let Some(encrypted) = &msg.encrypted_reasoning {
                    items.push(ResponsesInputItem::Reasoning {
                        id: None,
                        encrypted_content: encrypted.clone(),
                    });
                }

                // Assistant turnaround: Many proxies (like TokenX) break if a function_call item 
                // is sent without a preceding message item for the same assistant turn.
                if let Some(tool_calls) = &msg.tool_calls {
                    // Always emit a message item first, even if text content is empty, to anchor the function_calls
                    let content_str = msg.content_as_str();
                    items.push(ResponsesInputItem::Message {
                        role: "assistant".to_string(),
                        content: Some(ResponsesInputContent::Text(content_str.to_string())),
                    });
                    
                    for tc in tool_calls {
                        items.push(ResponsesInputItem::FunctionCall {
                            id: None, // Leave empty to let server assign its own item IDs
                            call_id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                        });
                    }
                } else {
                    items.push(ResponsesInputItem::Message {
                        role: "assistant".to_string(),
                        content: Some(ResponsesInputContent::Text(
                            msg.content_as_str().to_string(),
                        )),
                    });
                }
            }
            "tool" => {
                // Tool results map to function_call_output
                let call_id = msg
                    .tool_call_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                items.push(ResponsesInputItem::FunctionCallOutput {
                    call_id,
                    output: msg.content_as_str().to_string(),
                });
            }
            _ => {
                // Unknown roles are passed as-is with system fallback
                items.push(ResponsesInputItem::Message {
                    role: "system".to_string(),
                    content: Some(ResponsesInputContent::Text(
                        msg.content_as_str().to_string(),
                    )),
                });
            }
        }
    }

    (instructions, items)
}

/// Convert Responses API `output` items back to the internal `Message` format.
pub(crate) fn convert_responses_output_to_message(output: &[ResponsesOutputItem]) -> Message {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut reasoning_text = None;
    let mut encrypted_reasoning = None;

    for item in output {
        match item {
            ResponsesOutputItem::Message { content } => {
                for block in content {
                    match block {
                        ResponsesContentBlock::OutputText { text } => {
                            text_parts.push(text.clone());
                        }
                    }
                }
            }
            ResponsesOutputItem::Reasoning {
                summary,
                encrypted_content,
            } => {
                reasoning_text = Some(summary.clone());
                encrypted_reasoning = Some(encrypted_content.clone());
            }
            ResponsesOutputItem::FunctionCall {
                item_id,
                call_id,
                name,
                arguments,
            } => {
                tool_calls.push(ToolCall {
                    id: if !call_id.is_empty() {
                        call_id.clone()
                    } else {
                        item_id.clone()
                    },
                    r#type: "function".to_string(),
                    function: ToolFunction {
                        name: name.clone(),
                        arguments: arguments.clone(),
                    },
                });
            }
        }
    }

    let combined_text = text_parts.join("");
    Message {
        role: "assistant".to_string(),
        content: if combined_text.is_empty() {
            None
        } else {
            Some(Content::Simple(combined_text))
        },
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        tool_call_id: None,
        reasoning_text,
        encrypted_reasoning,
    }
}

// ===== OpenAiClient implementation =====

impl OpenAiClient {
    pub fn new(
        api_key: String,
        base_url: String,
        model: String,
        use_responses_api: bool,
    ) -> Self {
        Self {
            api_key: api_key.trim().to_string(),
            base_url: base_url.trim().to_string(),
            model: model.trim().to_string(),
            use_responses_api,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Value>>,
    ) -> Result<Message, String> {
        if self.use_responses_api {
            self.chat_responses_api(messages, tools).await
        } else {
            self.chat_completions_api(messages, tools).await
        }
    }

    /// Chat Completions API path (`/chat/completions`)
    async fn chat_completions_api(
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
            stream: false,
            tools,
        };

        let body_text = self.send_request(&url, &request).await?;

        // Defensive check: heuristic detection of legacy/malformed stream data
        if body_text.trim_start().starts_with("data:") {
            tracing::error!("Detected 'data:' prefix in response body, likely a stream");
            return Err("API error: Response body contains streaming data markers.".to_string());
        }

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

    /// Responses API path (`/responses`)
    async fn chat_responses_api(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Value>>,
    ) -> Result<Message, String> {
        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));

        let role_counts = messages.iter().fold(std::collections::HashMap::new(), |mut acc, m| {
            *acc.entry(&m.role).or_insert(0) += 1;
            acc
        });
        tracing::info!("AI Request [Responses API] | URL: {} | Model: {} | Messages: {} ({:?})",
            url, self.model, messages.len(), role_counts
        );

        let (instructions, input) = convert_messages_to_responses_input(&messages);

        // Transform tools: Chat Completions format wraps functions inside {"type": "function", "function": {...}},
        // Responses API expects the same wrapper but with "type": "function" at top level.
        let tools_for_responses = tools.map(|tv| {
            tv.into_iter()
                .map(|t| {
                    // If it already has "type": "function", keep as-is; otherwise wrap
                    if t.get("type").is_some() {
                        t
                    } else {
                        serde_json::json!({
                            "type": "function",
                            "function": t
                        })
                    }
                })
                .collect::<Vec<Value>>()
        });

        let request = ResponsesRequest {
            model: self.model.clone(),
            instructions,
            input,
            tools: tools_for_responses,
        };

        // Summary of payload for logs
        let item_types: Vec<String> = request.input.iter().map(|it| {
            match it {
                ResponsesInputItem::Message { role, .. } => format!("message({})", role),
                ResponsesInputItem::Reasoning { .. } => "reasoning".to_string(),
                ResponsesInputItem::FunctionCall { id, .. } => {
                    format!("function_call(id={})", id.as_deref().unwrap_or("none"))
                }
                ResponsesInputItem::FunctionCallOutput { call_id, .. } => format!("function_call_output({})", call_id),
            }
        }).collect();
        tracing::debug!("Responses API Input Sequence: [{}]", item_types.join(", "));

        let body_text = self.send_request(&url, &request).await?;

        let output_items = parse_responses_api_output(&body_text).map_err(|e| {
            let preview = if body_text.chars().count() > 500 {
                format!("{}...", body_text.chars().take(500).collect::<String>())
            } else {
                body_text.clone()
            };
            tracing::error!(
                "Responses API Parse Error: {} | Body Content: {}",
                e,
                preview
            );
            e
        })?;

        Ok(convert_responses_output_to_message(&output_items))
    }

    /// Common HTTP sending logic shared by both API paths
    async fn send_request<T: Serialize>(
        &self,
        url: &str,
        request: &T,
    ) -> Result<String, String> {
        let body = serde_json::to_string(request).map_err(|e| e.to_string())?;
        if body.chars().count() > 1000 {
            let preview: String = body.chars().take(1000).collect();
            tracing::debug!("Request Body Preview: {}...", preview);
        } else {
            tracing::debug!("Request Body: {}", body);
        }

        let response = self
            .http_client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Request Transport Error: {}", e);
                format!("Request failed: {}", e)
            })?;

        let status = response.status();
        let headers = response.headers().clone();
        tracing::info!("AI Response Status: {}", status);

        // Defensive check: If server sends event-stream, we must reject it early
        if let Some(ct) = headers.get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
            if ct.contains("text/event-stream") {
                tracing::error!("Server ignored stream:false and sent text/event-stream");
                return Err("API error: Received unexpected streaming response (text/event-stream).".to_string());
            }
        }

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

        tracing::debug!("AI Raw Response: {}", body_text);

        Ok(body_text)
    }
}
