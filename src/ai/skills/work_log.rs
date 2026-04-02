use crate::ai::skills::Skill;
use crate::ai::memory::MemoryManager;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::fs;
use std::path::PathBuf;
use chrono::{Local, Datelike, NaiveDate, Duration as ChronoDuration};

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

    fn get_merged_report_dir() -> PathBuf {
        let mut path = Self::get_log_dir();
        path.push("merged_reports");
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
         IMPORTANT: Before calling 'record_log', 'update_today_log', 'save_weekly_report', or 'merge_reports', you MUST first display the exact content you intend to save to the user and ask for their confirmation. Only proceed with the save action after the user explicitly approves. \
         - record_log: Call this to append a new work fragment to the daily log. Use it whenever the user asks to 'record' or 'log' something (Note: '#log' shortcut is handled by the system). \
         - update_today_log: Overwrites today's entire log with new content. Call this to modify or correct existing logs. You MUST use 'get_today_logs' to read the logs first before modifying, and then provide the fully updated markdown content. \
         - get_today_logs: Retrieves all log entries recorded today. Use this when the user wants to review what has been logged today. \
         - get_weekly_logs: Retrieves all log entries recorded from Monday to Sunday of the current week. This data serves as the foundation for synthesizing a weekly report. \
         - save_weekly_report: Persists a generated weekly report into the local 'reports' directory with an ISO-8601 name. \
         - merge_reports: Merges multiple weekly reports into a single consolidated report. Accepts 'merge_scope' ('month' or 'year'), optional 'target_year' (e.g. 2025, defaults to current year), and optional 'target_month' (1-12, defaults to current month, only used when scope is 'month'). The merged report is saved to the reports directory. \
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
            "update_today_log" => {
                let content = args["content"].as_str().ok_or("Missing 'content'")?;
                let now = Local::now();
                let date_str = now.format("%Y-%m-%d").to_string();
                let log_dir = Self::get_log_dir();
                
                fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
                let log_file = log_dir.join(format!("{}.md", date_str));
                
                fs::write(&log_file, content).map_err(|e| format!("Failed to update log: {}", e))?;
                
                Ok(format!("Log updated in {}.md", date_str))
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
                // Recent 100 items should provide enough context for "what I did"
                let history = self.memory.get_recent_history(100).map_err(|e| e.to_string())?;
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
            "get_today_logs" => {
                let now = Local::now();
                let date_str = now.format("%Y-%m-%d").to_string();
                let log_dir = Self::get_log_dir();
                let log_file = log_dir.join(format!("{}.md", date_str));

                if log_file.exists() {
                    let content = fs::read_to_string(&log_file)
                        .map_err(|e| format!("Failed to read today's log: {}", e))?;
                    Ok(format!("### {} logs\n{}", date_str, content))
                } else {
                    Ok(format!("No logs found for today ({}).", date_str))
                }
            }
            "merge_reports" => {
                let scope = args["merge_scope"].as_str().unwrap_or("month");
                let now = Local::now().date_naive();
                let target_year = args["target_year"].as_i64().unwrap_or(now.year() as i64) as i32;
                let target_month = args["target_month"].as_u64().unwrap_or(now.month() as u64) as u32;
                let report_dir = Self::get_report_dir();

                if !report_dir.exists() {
                    return Ok("No reports directory found. No weekly reports to merge.".to_string());
                }

                // Validate inputs
                if target_month < 1 || target_month > 12 {
                    return Err(format!("Invalid target_month: {}. Must be 1-12.", target_month));
                }

                // Determine date range based on scope
                let (range_start, range_end, merged_filename) = match scope {
                    "year" => {
                        let start = NaiveDate::from_ymd_opt(target_year, 1, 1)
                            .ok_or("Invalid date")?;
                        let end = NaiveDate::from_ymd_opt(target_year, 12, 31)
                            .ok_or("Invalid date")?;
                        let filename = format!("merged_year_{}.md", target_year);
                        (start, end, filename)
                    }
                    _ => { // default to "month"
                        let start = NaiveDate::from_ymd_opt(target_year, target_month, 1)
                            .ok_or("Invalid date")?;
                        // Last day of month: first day of next month - 1 day
                        let next_month_start = if target_month == 12 {
                            NaiveDate::from_ymd_opt(target_year + 1, 1, 1)
                        } else {
                            NaiveDate::from_ymd_opt(target_year, target_month + 1, 1)
                        }.ok_or("Invalid date")?;
                        let end = next_month_start - ChronoDuration::days(1);
                        let filename = format!("merged_month_{}-{:02}.md", target_year, target_month);
                        (start, end, filename)
                    }
                };

                // Scan report files matching "weekly_YYYY-MM-DD_to_YYYY-MM-DD.md"
                let mut merged_content = String::new();
                let mut report_count = 0u32;

                let mut entries: Vec<_> = fs::read_dir(&report_dir)
                    .map_err(|e| format!("Failed to read reports dir: {}", e))?
                    .filter_map(|e| e.ok())
                    .collect();
                entries.sort_by_key(|e| e.file_name());

                for entry in entries {
                    let fname = entry.file_name();
                    let fname_str = fname.to_string_lossy();
                    // Match pattern: weekly_YYYY-MM-DD_to_YYYY-MM-DD.md
                    if fname_str.starts_with("weekly_") && fname_str.ends_with(".md") {
                        let date_part = &fname_str[7..fname_str.len() - 3]; // strip "weekly_" and ".md"
                        let parts: Vec<&str> = date_part.split("_to_").collect();
                        if parts.len() == 2 {
                            if let (Ok(week_start), Ok(_week_end)) = (
                                NaiveDate::parse_from_str(parts[0], "%Y-%m-%d"),
                                NaiveDate::parse_from_str(parts[1], "%Y-%m-%d"),
                            ) {
                                // Include if the week starts within the range
                                if week_start >= range_start && week_start <= range_end {
                                    if let Ok(content) = fs::read_to_string(entry.path()) {
                                        merged_content.push_str(&format!(
                                            "---\n## Week: {} to {}\n{}\n",
                                            parts[0], parts[1], content
                                        ));
                                        report_count += 1;
                                    }
                                }
                            }
                        }
                    }
                }

                if merged_content.is_empty() {
                    return Ok(format!("No weekly reports found for scope '{}' (range: {} to {}).", scope, range_start, range_end));
                }

                // Save merged report to dedicated merged_reports directory
                let merged_dir = Self::get_merged_report_dir();
                fs::create_dir_all(&merged_dir).map_err(|e| e.to_string())?;
                let merged_file = merged_dir.join(&merged_filename);
                let header = format!("# Merged Report ({})\nScope: {} to {}\nReports included: {}\n\n",
                    scope, range_start, range_end, report_count);
                let full_content = format!("{}{}", header, merged_content);

                fs::write(&merged_file, &full_content)
                    .map_err(|e| format!("Failed to save merged report: {}", e))?;

                Ok(format!("Merged {} weekly reports into {}. Content preview:\n\n{}", report_count, merged_file.display(), full_content))
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
                            "enum": ["record_log", "update_today_log", "get_today_logs", "get_weekly_logs", "save_weekly_report", "merge_reports", "get_auto_suggest_context"],
                            "description": "Specific operation to perform: 'record_log' for single entries, 'update_today_log' for fully overwriting today's log, 'get_today_logs' for viewing today's records, 'get_weekly_logs' for aggregation, 'save_weekly_report' for storing results, 'merge_reports' for consolidating weekly reports by month or year, or 'get_auto_suggest_context' for smart recommendations."
                        },
                        "content": {
                            "type": "string",
                            "description": "The actual text content to be logged or the full body of the weekly report to be saved."
                        },
                        "merge_scope": {
                            "type": "string",
                            "enum": ["month", "year"],
                            "description": "Scope for the merge_reports action: 'month' to merge weekly reports of a specific month, 'year' to merge weekly reports of a specific year."
                        },
                        "target_year": {
                            "type": "integer",
                            "description": "Optional. The year to merge reports for (e.g. 2025). Defaults to the current year if not specified."
                        },
                        "target_month": {
                            "type": "integer",
                            "description": "Optional. The month (1-12) to merge reports for, only used when merge_scope is 'month'. Defaults to the current month if not specified."
                        }
                    },
                    "required": ["action"]
                }
            }
        })
    }
}
