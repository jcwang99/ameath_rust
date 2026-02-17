use crate::ai::skills::Skill;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

// --- Tavily Search Skill ---
pub struct TavilySearchSkill {
    api_key: String,
}

impl TavilySearchSkill {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl Skill for TavilySearchSkill {
    fn name(&self) -> &str {
        "tavily_search"
    }

    fn description(&self) -> &str {
        "Search the web using Tavily. Best for getting direct answers and high-quality search summaries. Use this as your primary search tool."
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
            .map_err(|e| format!("Tavily network error: {}", e))?;

        let res_json: Value = response
            .json()
            .await
            .map_err(|e| format!("Tavily parse error: {}", e))?;

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
                Ok(format!("Tavily results for '{}':\n{}", query, combined))
            }
        } else {
            Ok("No results found.".to_string())
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
                        "query": { "type": "string", "description": "The search query" }
                    },
                    "required": ["query"]
                }
            }
        })
    }
}

// --- Brave Search Skill ---
pub struct BraveSearchSkill {
    api_key: String,
}

impl BraveSearchSkill {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl Skill for BraveSearchSkill {
    fn name(&self) -> &str {
        "brave_search"
    }

    fn description(&self) -> &str {
        "Search the web using Brave Search. Provides AI-friendly descriptions and broad web coverage. Use this if Tavily fails or if you need a different perspective."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| "Missing 'query' argument".to_string())?;

        if self.api_key.is_empty() {
            return Err("Brave API key not configured".to_string());
        }

        let client = Client::new();
        let response = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &self.api_key)
            .query(&[("q", query), ("count", "5")])
            .send()
            .await
            .map_err(|e| format!("Brave network error: {}", e))?;

        let res_json: Value = response
            .json()
            .await
            .map_err(|e| format!("Brave parse error: {}", e))?;

        if let Some(results) = res_json
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
        {
            let combined = results
                .iter()
                .map(|r| {
                    let title = r
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("No Title");
                    let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let desc = r.get("description").and_then(|v| v.as_str()).unwrap_or("");
                    format!("- [{}]({}): {}", title, url, desc)
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            if combined.is_empty() {
                Ok("No results found.".to_string())
            } else {
                Ok(format!("Brave results for '{}':\n{}", query, combined))
            }
        } else {
            Ok("No results found.".to_string())
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
                        "query": { "type": "string", "description": "The search query" }
                    },
                    "required": ["query"]
                }
            }
        })
    }
}

// --- Firecrawl Scrape Skill ---
pub struct WebScrapeSkill {
    url: String,
    api_key: String,
}

impl WebScrapeSkill {
    pub fn new(url: String, api_key: String) -> Self {
        Self { url, api_key }
    }
}

#[async_trait]
impl Skill for WebScrapeSkill {
    fn name(&self) -> &str {
        "web_scrape"
    }

    fn description(&self) -> &str {
        "Scrape the full content of a specific URL using Firecrawl. Use this after finding a relevant URL via search if you need deeper details than the search snippet provides."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let target_url = args["url"]
            .as_str()
            .ok_or_else(|| "Missing 'url' argument".to_string())?;

        if self.url.is_empty() {
            return Err("Firecrawl URL not configured".to_string());
        }

        let client = Client::new();
        let api_url = format!("{}/v2/crawl", self.url.trim_end_matches('/'));

        let mut builder = client.post(&api_url).json(&json!({
            "url": target_url,
            "formats": ["markdown"]
        }));

        if !self.api_key.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = builder
            .send()
            .await
            .map_err(|e| format!("Firecrawl network error: {}", e))?;

        let res_json: Value = response
            .json()
            .await
            .map_err(|e| format!("Firecrawl parse error: {}", e))?;

        if let Some(data) = res_json.get("data") {
            if let Some(markdown) = data.get("markdown").and_then(|m| m.as_str()) {
                let truncated = if markdown.chars().count() > 2000 {
                    format!("{}...", markdown.chars().take(2000).collect::<String>())
                } else {
                    markdown.to_string()
                };
                return Ok(truncated);
            }
        } else if res_json
            .get("success")
            .and_then(|s| s.as_bool())
            .unwrap_or(false)
        {
            return Ok(
                "[Scrape job started successfully, but content is not available immediately]"
                    .to_string(),
            );
        }

        Err("Failed to extract content".to_string())
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
                        "url": { "type": "string", "description": "The URL to scrape" }
                    },
                    "required": ["url"]
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_names() {
        let tavily = TavilySearchSkill::new("key".to_string());
        let brave = BraveSearchSkill::new("key".to_string());
        let scrape = WebScrapeSkill::new("url".to_string(), "key".to_string());

        assert_eq!(tavily.name(), "tavily_search");
        assert_eq!(brave.name(), "brave_search");
        assert_eq!(scrape.name(), "web_scrape");
    }
}
