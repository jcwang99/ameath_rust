use crate::types::AiResponseMode;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn content_as_str(&self) -> String {
        self.content
            .as_ref()
            .map(|c| c.as_string())
            .unwrap_or_default()
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
    pub fn as_string(&self) -> String {
        match self {
            Content::Simple(s) => s.clone(),
            Content::Multimodal(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } => Some(text.as_str()),
                    ContentPart::ImageUrl { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
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
    response_mode: AiResponseMode,
    http_client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ClientStreamEvent {
    Start,
    TextDelta(String),
}

#[derive(Default)]
struct StreamingState {
    started: bool,
    role: Option<String>,
    content: String,
    tool_calls: BTreeMap<usize, PartialToolCall>,
}

#[derive(Default)]
struct PartialToolCall {
    id: String,
    call_type: String,
    function_name: String,
    arguments: String,
}

impl OpenAiClient {
    pub fn new(
        api_key: String,
        base_url: String,
        model: String,
        response_mode: AiResponseMode,
    ) -> Self {
        Self {
            api_key: api_key.trim().to_string(),
            base_url: base_url.trim().to_string(),
            model: model.trim().to_string(),
            response_mode,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn chat<F>(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Value>>,
        mut on_stream_event: F,
    ) -> Result<Message, String>
    where
        F: FnMut(ClientStreamEvent),
    {
        match self.response_mode {
            AiResponseMode::NonStreaming => {
                self.chat_once(messages, tools, false, &mut on_stream_event).await
            }
            AiResponseMode::Streaming => {
                self.chat_once(messages, tools, true, &mut on_stream_event).await
            }
            AiResponseMode::Auto => match self
                .chat_once(messages.clone(), tools.clone(), true, &mut on_stream_event)
                .await
            {
                Ok(message) => Ok(message),
                Err(stream_err) => {
                    tracing::warn!(
                        "Streaming request failed in auto mode, retrying non-stream: {}",
                        stream_err
                    );
                    self.chat_once(messages, tools, false, &mut on_stream_event)
                        .await
                }
            },
        }
    }

    async fn chat_once<F>(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Value>>,
        stream: bool,
        on_stream_event: &mut F,
    ) -> Result<Message, String>
    where
        F: FnMut(ClientStreamEvent),
    {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let role_counts = messages
            .iter()
            .fold(std::collections::HashMap::new(), |mut acc, m| {
                *acc.entry(&m.role).or_insert(0) += 1;
                acc
            });
        tracing::info!(
            "AI Request | URL: {} | Model: {} | Stream: {} | Messages: {} ({:?})",
            url,
            self.model,
            stream,
            messages.len(),
            role_counts
        );

        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            stream,
            tools,
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header(
                "Accept",
                if stream {
                    "text/event-stream, application/json"
                } else {
                    "application/json"
                },
            )
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Request Transport Error: {}", e);
                format!("Request failed: {}", e)
            })?;

        let status = response.status();
        let headers = response.headers().clone();
        tracing::info!("AI Response Status: {}", status);

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            tracing::error!("API Error Body: {}", error_text);
            return Err(format!("API Error ({}): {}", status, error_text));
        }

        let content_type = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if content_type.contains("text/event-stream") {
            return self.parse_streaming_response(response, on_stream_event).await;
        }

        let body_text = response.text().await.map_err(|e| {
            tracing::error!("Failed to get response text: {}", e);
            format!("Failed to get body: {}", e)
        })?;

        tracing::debug!("AI Raw Response: {}", body_text);

        if body_text.trim_start().starts_with("data:") {
            return parse_streaming_text_body(&body_text, on_stream_event);
        }

        parse_chat_response_text(&body_text)
    }

    async fn parse_streaming_response<F>(
        &self,
        response: reqwest::Response,
        on_stream_event: &mut F,
    ) -> Result<Message, String>
    where
        F: FnMut(ClientStreamEvent),
    {
        let mut state = StreamingState::default();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("Failed to read stream chunk: {}", e))?;
            let chunk_text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_text);

            while let Some(pos) = buffer.find('\n') {
                let line: String = buffer.drain(..=pos).collect();
                process_sse_line(line.trim_end_matches(['\r', '\n']), &mut state, on_stream_event)?;
            }
        }

        if !buffer.trim().is_empty() {
            process_sse_line(buffer.trim_end_matches(['\r', '\n']), &mut state, on_stream_event)?;
        }

        state.into_message()
    }
}

fn parse_streaming_text_body<F>(body_text: &str, on_stream_event: &mut F) -> Result<Message, String>
where
    F: FnMut(ClientStreamEvent),
{
    let mut state = StreamingState::default();
    for line in body_text.lines() {
        process_sse_line(line.trim(), &mut state, on_stream_event)?;
    }
    state.into_message()
}

fn process_sse_line<F>(
    line: &str,
    state: &mut StreamingState,
    on_stream_event: &mut F,
) -> Result<(), String>
where
    F: FnMut(ClientStreamEvent),
{
    if line.is_empty() || line.starts_with(':') {
        return Ok(());
    }

    if let Some(payload) = line.strip_prefix("data:") {
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            return Ok(());
        }

        let value: Value = serde_json::from_str(payload)
            .map_err(|e| format!("Failed to parse streaming payload: {}", e))?;
        apply_stream_delta(&value, state, on_stream_event)
    } else {
        Ok(())
    }
}

fn apply_stream_delta<F>(
    value: &Value,
    state: &mut StreamingState,
    on_stream_event: &mut F,
) -> Result<(), String>
where
    F: FnMut(ClientStreamEvent),
{
    let delta = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .ok_or_else(|| "Streaming response missing choices[0].delta".to_string())?;

    if let Some(role) = delta.get("role").and_then(Value::as_str) {
        state.role = Some(role.to_string());
    }

    if let Some(content_value) = delta.get("content") {
        let text_delta = extract_text_from_content_value(content_value);
        if !text_delta.is_empty() {
            if !state.started {
                state.started = true;
                on_stream_event(ClientStreamEvent::Start);
            }
            state.content.push_str(&text_delta);
            on_stream_event(ClientStreamEvent::TextDelta(text_delta));
        }
    }

    if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            let index = tool_call
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or(state.tool_calls.len() as u64) as usize;

            let entry = state.tool_calls.entry(index).or_default();

            if let Some(id) = tool_call.get("id").and_then(Value::as_str) {
                entry.id = id.to_string();
            }
            if let Some(call_type) = tool_call.get("type").and_then(Value::as_str) {
                entry.call_type = call_type.to_string();
            }
            if let Some(function) = tool_call.get("function") {
                if let Some(name) = function.get("name").and_then(Value::as_str) {
                    entry.function_name = name.to_string();
                }
                if let Some(arguments) = function.get("arguments") {
                    entry.arguments.push_str(&value_to_argument_string(arguments));
                }
            }
        }
    }

    Ok(())
}

impl StreamingState {
    fn into_message(self) -> Result<Message, String> {
        let tool_calls = if self.tool_calls.is_empty() {
            None
        } else {
            Some(
                self.tool_calls
                    .into_values()
                    .map(|partial| ToolCall {
                        id: if partial.id.is_empty() {
                            format!("call_{}", partial.function_name)
                        } else {
                            partial.id
                        },
                        r#type: if partial.call_type.is_empty() {
                            "function".to_string()
                        } else {
                            partial.call_type
                        },
                        function: ToolFunction {
                            name: partial.function_name,
                            arguments: if partial.arguments.is_empty() {
                                "{}".to_string()
                            } else {
                                partial.arguments
                            },
                        },
                    })
                    .collect(),
            )
        };

        let content = if self.content.is_empty() {
            None
        } else {
            Some(Content::Simple(self.content))
        };

        if content.is_none() && tool_calls.is_none() {
            return Err("No response from AI".to_string());
        }

        Ok(Message {
            role: self.role.unwrap_or_else(|| "assistant".to_string()),
            content,
            tool_calls,
            tool_call_id: None,
        })
    }
}

pub(crate) fn parse_chat_response_text(body_text: &str) -> Result<Message, String> {
    let value: Value = serde_json::from_str(body_text).map_err(|e| {
        let preview = if body_text.chars().count() > 500 {
            format!("{}...", body_text.chars().take(500).collect::<String>())
        } else {
            body_text.to_string()
        };
        tracing::error!("JSON Parse Error: {} | Body Content: {}", e, preview);
        format!("Failed to parse response: {}", e)
    })?;

    parse_message_from_value(&value)
}

pub(crate) fn parse_message_from_value(value: &Value) -> Result<Message, String> {
    if let Some(message) = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
    {
        return parse_message_object(message);
    }

    if let Some(message) = value.get("message") {
        return parse_message_object(message);
    }

    Err("No response from AI".to_string())
}

fn parse_message_object(message: &Value) -> Result<Message, String> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("assistant")
        .to_string();

    let content = normalize_content(message.get("content"));
    let tool_calls = normalize_tool_calls(message.get("tool_calls"));
    let tool_call_id = message
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::to_string);

    if content.is_none() && tool_calls.is_none() {
        return Err("No response from AI".to_string());
    }

    Ok(Message {
        role,
        content,
        tool_calls,
        tool_call_id,
    })
}

fn normalize_content(content: Option<&Value>) -> Option<Content> {
    let value = content?;
    if value.is_null() {
        return None;
    }

    if let Some(text) = value.as_str() {
        return Some(Content::Simple(text.to_string()));
    }

    let parts = normalize_content_parts(value);
    if parts.is_empty() {
        None
    } else {
        Some(Content::Multimodal(parts))
    }
}

fn normalize_content_parts(value: &Value) -> Vec<ContentPart> {
    let values = match value {
        Value::Array(items) => items.iter().collect::<Vec<_>>(),
        _ => vec![value],
    };

    let mut parts = Vec::new();
    for item in values {
        if let Some(text) = item.as_str() {
            parts.push(ContentPart::Text {
                text: text.to_string(),
            });
            continue;
        }

        if let Some(obj) = item.as_object() {
            let item_type = obj.get("type").and_then(Value::as_str).unwrap_or("text");
            match item_type {
                "text" | "output_text" => {
                    if let Some(text) = obj.get("text").and_then(Value::as_str) {
                        parts.push(ContentPart::Text {
                            text: text.to_string(),
                        });
                    }
                }
                "image_url" => {
                    let url = obj
                        .get("image_url")
                        .and_then(|v| v.get("url").or(Some(v)))
                        .and_then(Value::as_str);
                    if let Some(url) = url {
                        parts.push(ContentPart::ImageUrl {
                            image_url: ImageUrl {
                                url: url.to_string(),
                            },
                        });
                    }
                }
                _ => {
                    if let Some(text) = obj.get("text").and_then(Value::as_str) {
                        parts.push(ContentPart::Text {
                            text: text.to_string(),
                        });
                    }
                }
            }
        }
    }

    parts
}

fn normalize_tool_calls(tool_calls: Option<&Value>) -> Option<Vec<ToolCall>> {
    let items = tool_calls?.as_array()?;
    let mut normalized = Vec::new();

    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let call_type = item
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("function")
            .to_string();
        let function = item.get("function").cloned().unwrap_or(Value::Null);
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let arguments = function
            .get("arguments")
            .map(value_to_argument_string)
            .unwrap_or_else(|| "{}".to_string());

        normalized.push(ToolCall {
            id,
            r#type: call_type,
            function: ToolFunction { name, arguments },
        });
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn extract_text_from_content_value(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    normalize_content_parts(value)
        .into_iter()
        .filter_map(|part| match part {
            ContentPart::Text { text } => Some(text),
            ContentPart::ImageUrl { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn value_to_argument_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
