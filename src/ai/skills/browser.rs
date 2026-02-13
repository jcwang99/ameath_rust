use crate::ai::skills::Skill;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

pub struct BrowserSkill {
    api_key: String,
}

impl BrowserSkill {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl Skill for BrowserSkill {
    fn name(&self) -> &str {
        "browser_search"
    }

    fn description(&self) -> &str {
        "Search the web for real-time information, news, or specific facts using Tavily."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| "Missing 'query' argument".to_string())?;

        if self.api_key.is_empty() {
            return Err("Tavily API key not configured".to_string());
        }

        let client = Client::new();
        let response = client
            .post("https://api.tavily.com/search")
            .json(&json!({
                "api_key": self.api_key,
                "query": query,
                "search_depth": "basic",
                "include_answer": true,
                "max_results": 5
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        let res_json: Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Return the 'answer' field if available, or a summary of results
        if let Some(answer) = res_json.get("answer").and_then(|a| a.as_str()) {
            Ok(answer.to_string())
        } else if let Some(results) = res_json.get("results").and_then(|r| r.as_array()) {
            let combined = results
                .iter()
                .take(5)
                .map(|r| {
                    let title = r
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("No Title");
                    let content = r
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("No Content");
                    let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    format!("- [{}]({}): {}", title, url, content)
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            if combined.is_empty() {
                Ok("No results found.".to_string())
            } else {
                Ok(format!("Search results for '{}':\n{}", query, combined))
            }
        } else {
            // Check if there is an error message
            if let Some(error) = res_json
                .get("detail")
                .and_then(|d| d.get("error"))
                .and_then(|e| e.as_str())
            {
                Err(format!("Tavily API Error: {}", error))
            } else {
                Ok("No results found.".to_string())
            }
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
                        "query": {
                            "type": "string",
                            "description": "The search query"
                        }
                    },
                    "required": ["query"]
                }
            }
        })
    }
}
