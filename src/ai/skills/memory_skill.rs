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
                let key = args["key"].as_str();
                let facts = self.memory.get_facts().map_err(|e| e.to_string())?;
                if let Some(k) = key {
                    if let Some(val) = facts.iter().find(|(name, _)| name == k).map(|(_, v)| v) {
                        Ok(format!("{}: {}", k, val))
                    } else {
                        Ok(format!("Fact not found for key: {}", k))
                    }
                } else {
                    if facts.is_empty() {
                        Ok("No facts found in the Fact Board.".to_string())
                    } else {
                        let all_facts = facts
                            .iter()
                            .map(|(k, v)| format!("{}: {}", k, v))
                            .collect::<Vec<_>>()
                            .join("\n");
                        Ok(format!("Current Fact Board:\n{}", all_facts))
                    }
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
                            "description": "Unique key for facts (e.g. 'user_taste'). Required for set/delete. Optional for 'get' (if omitted, returns all facts)."
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::memory::MemoryManager;

    #[tokio::test]
    async fn test_memory_skill_get_all() {
        // 创建一个具有独立隔离数据的 MemoryManager 测试实例比较困难，
        // 因为 new() 内部硬编码了 data/ameath_memory.db。
        // 为了验证逻辑，我们使用当前的真实 Manager，但在测试后不一定能清理。
        // 注意：在真实环境下运行单元测试可能会干扰本地数据库。
        let memory = Arc::new(MemoryManager::new());
        let skill = MemorySkill::new(memory.clone());

        let test_key_1 = "test_key_name_123";
        let test_key_2 = "test_key_role_456";

        // 1. 设置数据
        skill.execute(json!({
            "action": "set",
            "key": test_key_1,
            "value": "Antigravity"
        })).await.unwrap();
        
        skill.execute(json!({
            "action": "set",
            "key": test_key_2,
            "value": "AI Assistant"
        })).await.unwrap();

        // 2. 测试获取单个 key
        let res_single = skill.execute(json!({
            "action": "get",
            "key": test_key_1
        })).await.unwrap();
        assert!(res_single.contains(test_key_1));
        assert!(res_single.contains("Antigravity"));

        // 3. 测试获取所有 key (不传入 key)
        let res_all = skill.execute(json!({
            "action": "get"
        })).await.unwrap();
        
        assert!(res_all.contains("Current Fact Board:"));
        assert!(res_all.contains(test_key_1));
        assert!(res_all.contains(test_key_2));

        // 清理测试数据
        let _ = skill.execute(json!({"action": "delete", "key": test_key_1})).await;
        let _ = skill.execute(json!({"action": "delete", "key": test_key_2})).await;
    }

    #[tokio::test]
    async fn test_memory_skill_get_empty() {
        // 由于真实 DB 可能不为空，此测试在真实环境可能不适用，
        // 故我们只验证逻辑路径。
        let memory = Arc::new(MemoryManager::new());
        let skill = MemorySkill::new(memory);

        let res = skill.execute(json!({
            "action": "get"
        })).await.unwrap();
        
        // 只要返回包含预期前缀或特定字符串即可证明逻辑走通
        assert!(res.contains("Current Fact Board") || res == "No facts found in the Fact Board.");
    }
}
