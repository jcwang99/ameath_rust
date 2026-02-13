use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct PreprocessedFrame {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
    pub delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PetState {
    Idle,
    Move,
    Drag,
    Clingy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_react_limit")]
    pub react_limit: usize,
    #[serde(default = "default_l1_threshold")]
    pub l1_summary_threshold: usize,
    #[serde(default = "default_l2_threshold")]
    pub l2_merge_threshold: usize,
    #[serde(default = "String::new")]
    pub tavily_api_key: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

fn default_react_limit() -> usize {
    20
}
fn default_l1_threshold() -> usize {
    10
}
fn default_l2_threshold() -> usize {
    10
}
fn default_system_prompt() -> String {
    "You are Ameath, a desktop pet assistant. You are helpful, witty, and slightly mischievous."
        .to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            model: "deepseek-chat".to_string(),
            react_limit: 20,
            l1_summary_threshold: 10,
            l2_merge_threshold: 10,
            tavily_api_key: String::new(),
            system_prompt: default_system_prompt(),
        }
    }
}

impl AiConfig {
    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(config) = serde_json::from_str::<Self>(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, content);
        }
    }

    fn path() -> PathBuf {
        let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        path.pop();
        path.push("ai_config.json");
        path
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorMode {
    Quiet,
    Active,
    Clingy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowLayer {
    Top,
    Bottom,
}
