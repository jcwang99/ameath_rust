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

fn default_time() -> DateTime<Local> {
    Local::now()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Todo {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    #[serde(default = "default_time")]
    pub created_at: DateTime<Local>,
    pub completed_at: Option<DateTime<Local>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum TodoStatus {
    Pending,
    Done,
}

pub struct TodoSkill {
    memory: Arc<MemoryManager>,
}

impl TodoSkill {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }

    fn get_todo_file() -> PathBuf {
        let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        path.push("data");
        path.push("todos.json");
        path
    }

    fn load_todos() -> Vec<Todo> {
        let file = Self::get_todo_file();
        if !file.exists() {
            return Vec::new();
        }
        let content = fs::read_to_string(file).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    }

    fn save_todos(todos: &[Todo]) -> Result<(), String> {
        let file = Self::get_todo_file();
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let content = serde_json::to_string_pretty(todos).map_err(|e| e.to_string())?;
        fs::write(file, content).map_err(|e| e.to_string())
    }

    pub fn add_todo_local(content: &str) -> Result<String, String> {
        let mut todos = Self::load_todos();
        let new_todo = Todo {
            id: Uuid::new_v4().to_string(),
            content: content.to_string(),
            status: TodoStatus::Pending,
            created_at: Local::now(),
            completed_at: None,
        };
        todos.push(new_todo);
        Self::save_todos(&todos)?;
        Ok(format!("Todo added: {}", content))
    }

    pub fn list_todos_local(only_pending: bool) -> Vec<Todo> {
        let todos = Self::load_todos();
        if only_pending {
            todos.into_iter().filter(|t| t.status == TodoStatus::Pending).collect()
        } else {
            todos
        }
    }

    pub fn complete_todo_local(id_prefix: &str) -> Result<String, String> {
        let mut todos = Self::load_todos();
        let mut found_index = None;

        for (i, todo) in todos.iter().enumerate() {
            if todo.id.starts_with(id_prefix) && todo.status == TodoStatus::Pending {
                found_index = Some(i);
                break;
            }
        }

        if let Some(idx) = found_index {
            todos[idx].status = TodoStatus::Done;
            todos[idx].completed_at = Some(Local::now());
            let content = todos[idx].content.clone();
            Self::save_todos(&todos)?;
            
            // Link with WorkLogSkill: Record to log automatically
            let _ = crate::ai::skills::work_log::WorkLogSkill::record_log_local(&format!("[Todo Done] {}", content));
            
            Ok(format!("Completed: {}", content))
        } else {
            Err("No matching pending todo found.".to_string())
        }
    }
}

#[async_trait]
impl Skill for TodoSkill {
    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        "Manages a todo list. \
         - add_todo: Adds a new task. \
         - list_todos: Lists current pending tasks (will NOT show completed ones). \
         - list_completed_todos: Lists recently finished tasks. \
         - complete_todo: Marks a todo as finished by its ID prefix. \
         - delete_todo: Removes a todo. \
         Shortcut: Use '#todo <task>' in chat for quick adding without LLM."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let action = args["action"].as_str().ok_or("Missing action")?;
        match action {
            "add_todo" => {
                let content = args["content"].as_str().ok_or("Missing content")?;
                Self::add_todo_local(content)
            }
            "list_todos" => {
                let todos = Self::list_todos_local(true);
                if todos.is_empty() {
                    Ok("No pending todos found.".to_string())
                } else {
                    let mut list = String::from("Current Pending Todos:\n");
                    for todo in todos {
                        list.push_str(&format!("- [ ] ({}) {} (Added: {})\n", 
                            &todo.id[..4], 
                            todo.content,
                            todo.created_at.format("%Y-%m-%d %H:%M")
                        ));
                    }
                    Ok(list)
                }
            }
            "list_completed_todos" => {
                let todos = Self::load_todos();
                let completed: Vec<_> = todos.into_iter().filter(|t| t.status == TodoStatus::Done).collect();
                if completed.is_empty() {
                    Ok("No completed todos found.".to_string())
                } else {
                    let mut list = String::from("Recently Completed Todos:\n");
                    for todo in completed {
                        let comp_time = todo.completed_at.map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| "N/A".to_string());
                        list.push_str(&format!("- [x] ({}) {} (Done at: {})\n", 
                            &todo.id[..4], 
                            todo.content,
                            comp_time
                        ));
                    }
                    Ok(list)
                }
            }
            "complete_todo" => {
                let id = args["id"].as_str().ok_or("Missing id")?;
                Self::complete_todo_local(id)
            }
            "delete_todo" => {
                let id = args["id"].as_str().ok_or("Missing id")?;
                let mut todos = Self::load_todos();
                let initial_len = todos.len();
                todos.retain(|t| !t.id.starts_with(id));
                if todos.len() < initial_len {
                    Self::save_todos(&todos)?;
                    Ok("Todo deleted.".to_string())
                } else {
                    Err("Todo not found.".to_string())
                }
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
                            "enum": ["add_todo", "list_todos", "list_completed_todos", "complete_todo", "delete_todo"]
                        },
                        "content": {
                            "type": "string",
                            "description": "Content of the todo (required for add_todo)"
                        },
                        "id": {
                            "type": "string",
                            "description": "ID or ID prefix of the todo (required for complete/delete)"
                        }
                    },
                    "required": ["action"]
                }
            }
        })
    }
}
