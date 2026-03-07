use crate::ai::skills::Skill;
use crate::ai::memory::MemoryManager;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::fs;
use std::path::PathBuf;
use chrono::{Local, Datelike, Duration as ChronoDuration};

pub struct WorkLogSkill {
    memory: Arc<MemoryManager>,
}

impl WorkLogSkill {
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }

    fn get_log_dir() -> PathBuf {
        let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        path.push("data");
        path.push("logs");
        path
    }

    fn get_report_dir() -> PathBuf {
        let mut path = Self::get_log_dir();
        path.push("reports");
        path
    }

    pub fn record_log_local(content: &str) -> Result<String, String> {
        let now = Local::now();
        let date_str = now.format("%Y-%m-%d").to_string();
        let time_str = now.format("%H:%M").to_string();
        
        let log_dir = Self::get_log_dir();
        fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
        
        let log_file = log_dir.join(format!("{}.md", date_str));
        let log_line = format!("- [{}] {}\n", time_str, content);
        
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .and_then(|mut f| {
                use std::io::Write;
                f.write_all(log_line.as_bytes())?;
                f.flush()
            })
            .map_err(|e| format!("Failed to write log: {}", e))?;
        
        Ok(format!("Log recorded in {}.md", date_str))
    }
}

#[async_trait]
impl Skill for WorkLogSkill {
    fn name(&self) -> &str {
        "work_log"
    }

    fn description(&self) -> &str {
        "A specialized skill for automating work logging and professional weekly reporting. \
         - record_log: Call this to append a new work fragment to the daily log. Use it whenever the user asks to 'record' or 'log' something (Note: '#log' shortcut is handled by the system). \
         - get_weekly_logs: Retrieves all log entries recorded from Monday to Sunday of the current week. This data serves as the foundation for synthesizing a weekly report. \
         - save_weekly_report: Persists a generated weekly report into the local 'reports' directory with an ISO-8601 name. \
         - get_auto_suggest_context: Gathers context by comparing recorded logs against system activity history. Trigger this when the user requests a 'retrospection' or asks 'what did I miss today?' to find unrecorded tasks."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| "Missing 'action'".to_string())?;

        match action {
            "record_log" => {
                let content = args["content"].as_str().ok_or("Missing 'content'")?;
                Self::record_log_local(content)
            }
            "get_weekly_logs" => {
                let now = Local::now().date_naive();
                // weekday() returns 0-6 (Mon-Sun) in chrono 0.4.x with Datelike (assuming ISO week or similar)
                // Actually chrono's weekday() returns an enum. 
                let weekday = now.weekday().number_from_monday(); // 1 (Mon) to 7 (Sun)
                let monday = now - ChronoDuration::days((weekday - 1) as i64);
                
                let mut weekly_content = String::new();
                let log_dir = Self::get_log_dir();

                for i in 0..7 {
                    let date = monday + ChronoDuration::days(i);
                    let date_str = date.format("%Y-%m-%d").to_string();
                    let log_file = log_dir.join(format!("{}.md", date_str));
                    
                    if log_file.exists() {
                        if let Ok(content) = fs::read_to_string(&log_file) {
                            weekly_content.push_str(&format!("### {}\n{}\n", date_str, content));
                        }
                    }
                }
                
                if weekly_content.is_empty() {
                    Ok("No logs found for this week.".to_string())
                } else {
                    Ok(weekly_content)
                }
            }
            "save_weekly_report" => {
                let report_content = args["content"].as_str().ok_or("Missing 'content'")?;
                let now = Local::now().date_naive();
                let weekday = now.weekday().number_from_monday();
                let monday = now - ChronoDuration::days((weekday - 1) as i64);
                let sunday = monday + ChronoDuration::days(6);
                
                let report_dir = Self::get_report_dir();
                fs::create_dir_all(&report_dir).map_err(|e| e.to_string())?;
                
                let filename = format!("weekly_{}_to_{}.md", monday.format("%Y-%m-%d"), sunday.format("%Y-%m-%d"));
                let report_file = report_dir.join(filename);
                
                fs::write(&report_file, report_content).map_err(|e| format!("Failed to save report: {}", e))?;
                
                Ok(format!("Weekly report saved to {}", report_file.display()))
            }
            "get_auto_suggest_context" => {
                // Get today's logs
                let now = Local::now();
                let date_str = now.format("%Y-%m-%d").to_string();
                let log_dir = Self::get_log_dir();
                let log_file = log_dir.join(format!("{}.md", date_str));
                
                let today_logs = if log_file.exists() {
                    fs::read_to_string(&log_file).unwrap_or_default()
                } else {
                    "None".to_string()
                };
                
                // Get recent history from memory as "system activity"
                // Recent 20 items should provide enough context for "what I did"
                let history = self.memory.get_recent_history(20).map_err(|e| e.to_string())?;
                let system_activity = history.iter()
                    .map(|(role, content)| format!("[{}] {}", role, content))
                    .collect::<Vec<_>>()
                    .join("\n");

                let result = json!({
                    "today_logs": today_logs,
                    "system_activity_data": system_activity
                });

                Ok(result.to_string())
            }
            _ => Err(format!("Unknown action: {}", action))
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
                            "enum": ["record_log", "get_weekly_logs", "save_weekly_report", "get_auto_suggest_context"],
                            "description": "Specific operation to perform: 'record_log' for single entries, 'get_weekly_logs' for aggregation, 'save_weekly_report' for storing results, or 'get_auto_suggest_context' for smart recommendations."
                        },
                        "content": {
                            "type": "string",
                            "description": "The actual text content to be logged or the full body of the weekly report to be saved."
                        }
                    },
                    "required": ["action"]
                }
            }
        })
    }
}
