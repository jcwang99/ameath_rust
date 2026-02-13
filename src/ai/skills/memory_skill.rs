use crate::ai::memory::MemoryManager;
use crate::ai::skills::Skill;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct MemorySkill {
    memory: Arc<MemoryManager>,
}

impl MemorySkill {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Skill for MemorySkill {
    fn name(&self) -> &str {
        "memory_manage"
    }

    fn description(&self) -> &str {
        "Manage long-term facts and user preferences in the persistent Fact Board. Use this to store key information mentioned by the user or retrieve specialized facts."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| "Missing 'action' (set/get)".to_string())?;

        match action {
            "set" => {
                let key = args["key"].as_str().ok_or("Missing 'key'")?;
                let value = args["value"].as_str().ok_or("Missing 'value'")?;
                self.memory
                    .set_fact(key, value)
                    .map_err(|e| e.to_string())?;
                Ok(format!("Fact stored: {} = {}", key, value))
            }
            "get" => {
                let key = args["key"].as_str().ok_or("Missing 'key'")?;
                let facts = self.memory.get_facts().map_err(|e| e.to_string())?;
                if let Some(val) = facts.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
                    Ok(format!("{}: {}", key, val))
                } else {
                    Ok(format!("Fact not found for key: {}", key))
                }
            }
            _ => Err("Invalid action. Use 'set' or 'get'.".to_string()),
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
                            "enum": ["set", "get"],
                            "description": "The action to perform: 'set' to store a fact, 'get' to retrieve one."
                        },
                        "key": {
                            "type": "string",
                            "description": "The unique key for the fact (e.g., 'user_name', 'favorite_color')"
                        },
                        "value": {
                            "type": "string",
                            "description": "The value to store (required for 'set')"
                        }
                    },
                    "required": ["action", "key"]
                }
            }
        })
    }
}
