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
        "update_fact_board"
    }

    fn description(&self) -> &str {
        "Maintain the core Fact Board and Knowledge Graph about the user. Use this to permanently 'learn' and 'set' important user preferences or facts, 'delete' obsolete facts, OR build relationship graphs using 'add_relation' (e.g. Alice -> is_friend_of -> Bob) and 'delete_relation'."
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
            "delete" => {
                let key = args["key"].as_str().ok_or("Missing 'key'")?;
                self.memory.delete_fact(key).map_err(|e| e.to_string())?;
                Ok(format!("Fact deleted: {}", key))
            }
            "add_relation" => {
                let source = args["source"].as_str().ok_or("Missing 'source'")?;
                let relation = args["relation"].as_str().ok_or("Missing 'relation'")?;
                let target = args["target"].as_str().ok_or("Missing 'target'")?;
                self.memory
                    .add_relation(source, relation, target)
                    .map_err(|e| e.to_string())?;
                Ok(format!(
                    "Relation stored: {} -[{}]-> {}",
                    source, relation, target
                ))
            }
            "delete_relation" => {
                let source = args["source"].as_str().ok_or("Missing 'source'")?;
                let relation = args["relation"].as_str().ok_or("Missing 'relation'")?;
                let target = args["target"].as_str().ok_or("Missing 'target'")?;
                self.memory
                    .delete_relation(source, relation, target)
                    .map_err(|e| e.to_string())?;
                Ok(format!(
                    "Relation deleted: {} -[{}]-> {}",
                    source, relation, target
                ))
            }
            _ => Err(
                "Invalid action. Use 'set', 'get', 'delete', 'add_relation', or 'delete_relation'."
                    .to_string(),
            ),
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
                            "enum": ["set", "get", "delete", "add_relation", "delete_relation"],
                            "description": "The action: 'set/get/delete' for flat facts, 'add_relation/delete_relation' for graph triplets."
                        },
                        "key": {
                            "type": "string",
                            "description": "Unique key for facts (e.g. 'user_taste'). Required for set/get/delete."
                        },
                        "value": {
                            "type": "string",
                            "description": "The definitive fact to record. Required for 'set'."
                        },
                        "source": {
                            "type": "string",
                            "description": "Source entity for graph (e.g. 'Alice'). Required for add_relation/delete_relation."
                        },
                        "relation": {
                            "type": "string",
                            "description": "Relationship (e.g. 'is_friend_of'). Required for add_relation/delete_relation."
                        },
                        "target": {
                            "type": "string",
                            "description": "Target entity for graph (e.g. 'Bob'). Required for add_relation/delete_relation."
                        }
                    },
                    "required": ["action"]
                }
            }
        })
    }
}
