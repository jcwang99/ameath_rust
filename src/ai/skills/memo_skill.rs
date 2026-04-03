use crate::ai::skills::Skill;
use crate::ai::memory::MemoryManager;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::fs;
use std::path::PathBuf;
use chrono::{Local, DateTime};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Memo {
    pub id: String,
    pub content: String,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}

pub struct MemoSkill {
    #[allow(dead_code)]
    memory: Arc<MemoryManager>,
}

impl MemoSkill {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }

    fn get_memo_file() -> PathBuf {
        let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        path.push("data");
        path.push("memos.json");
        path
    }

    fn load_memos() -> Vec<Memo> {
        let file = Self::get_memo_file();
        if !file.exists() {
            return Vec::new();
        }
        let content = fs::read_to_string(file).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    }

    fn save_memos(memos: &[Memo]) -> Result<(), String> {
        let file = Self::get_memo_file();
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let content = serde_json::to_string_pretty(memos).map_err(|e| e.to_string())?;
        fs::write(file, content).map_err(|e| e.to_string())
    }

    pub fn add_memo_local(content: &str) -> Result<String, String> {
        let mut memos = Self::load_memos();
        let now = Local::now();
        let new_memo = Memo {
            id: Uuid::new_v4().to_string(),
            content: content.to_string(),
            created_at: now,
            updated_at: now,
        };
        memos.push(new_memo);
        Self::save_memos(&memos)?;
        Ok(format!("Memo added: {}", content))
    }

    pub fn list_memos_local() -> Vec<Memo> {
        Self::load_memos()
    }

    pub fn update_memo_local(id_prefix: &str, new_content: &str) -> Result<String, String> {
        let mut memos = Self::load_memos();
        if let Some(memo) = memos.iter_mut().find(|m| m.id.starts_with(id_prefix)) {
            memo.content = new_content.to_string();
            memo.updated_at = Local::now();
            Self::save_memos(&memos)?;
            Ok(format!("Memo updated: {}", new_content))
        } else {
            Err("Memo not found.".to_string())
        }
    }

    pub fn delete_memo_local(id_prefix: &str) -> Result<String, String> {
        let mut memos = Self::load_memos();
        let initial_len = memos.len();
        memos.retain(|m| !m.id.starts_with(id_prefix));
        if memos.len() < initial_len {
            Self::save_memos(&memos)?;
            Ok("Memo deleted.".to_string())
        } else {
            Err("Memo not found.".to_string())
        }
    }
}

#[async_trait]
impl Skill for MemoSkill {
    fn name(&self) -> &str {
        "memo"
    }

    fn description(&self) -> &str {
        "Manages personal memos (reminders of things to not forget). \
         - add_memo: Records a new memo. \
         - list_memos: Lists all existing memos. \
         - update_memo: Updates an existing memo by its ID prefix. \
         - delete_memo: Removes a memo by its ID prefix. \
         Shortcut: Use '#memo <text>' in chat for quick recording."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let action = args["action"].as_str().ok_or("Missing action")?;
        match action {
            "add_memo" => {
                let content = args["content"].as_str().ok_or("Missing content")?;
                Self::add_memo_local(content)
            }
            "list_memos" => {
                let memos = Self::list_memos_local();
                if memos.is_empty() {
                    Ok("No memos found.".to_string())
                } else {
                    let mut list = String::from("Your Memos:\n");
                    for m in memos {
                        list.push_str(&format!("- [{}] {}\n", &m.id[..4], m.content));
                    }
                    Ok(list)
                }
            }
            "update_memo" => {
                let id = args["id"].as_str().ok_or("Missing id")?;
                let content = args["content"].as_str().ok_or("Missing content")?;
                Self::update_memo_local(id, content)
            }
            "delete_memo" => {
                let id = args["id"].as_str().ok_or("Missing id")?;
                Self::delete_memo_local(id)
            }
            _ => Err(format!("Unknown action: {}", action)),
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
                        "action": {
                            "type": "string",
                            "enum": ["add_memo", "list_memos", "update_memo", "delete_memo"]
                        },
                        "content": {
                            "type": "string",
                            "description": "Content of the memo (required for add/update)"
                        },
                        "id": {
                            "type": "string",
                            "description": "ID or ID prefix of the memo (required for update/delete)"
                        }
                    },
                    "required": ["action"]
                }
            }
        })
    }
}
