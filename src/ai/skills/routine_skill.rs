use crate::ai::skills::Skill;
use crate::types::{PersistentConfig, RoutineDef, RoutinesConfig, ScheduleType};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub struct RoutineSkill {
    config: Arc<Mutex<RoutinesConfig>>,
}

impl RoutineSkill {
    pub fn new(config: Arc<Mutex<RoutinesConfig>>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Skill for RoutineSkill {
    fn name(&self) -> &str {
        "manage_routine"
    }

    fn description(&self) -> &str {
        "Manage periodic scheduled routines. Supports 'add', 'list', and 'remove' actions. CRITICAL: Before removing, you MUST call 'list' first to get the exact ID. Never guess or assume an ID — wrong deletions cannot be undone. Max 20 routines."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let action = args["action"]
            .as_str()
            .ok_or("Missing 'action' (string: 'add', 'list', or 'remove')")?;

        let mut cfg = self.config.lock().unwrap();

        match action {
            "list" => {
                if cfg.routines.is_empty() {
                    return Ok("No routines configured.".to_string());
                }
                let mut out = String::new();
                for (i, r) in cfg.routines.iter().enumerate() {
                    let schedule = match r.schedule_type {
                        ScheduleType::Daily => format!("Daily at {}", r.time_of_day.as_deref().unwrap_or("?")),
                        ScheduleType::Weekly => format!("Weekly day={} at {}", r.day_of_week.unwrap_or(0), r.time_of_day.as_deref().unwrap_or("?")),
                        ScheduleType::Monthly => format!("Monthly day={} at {}", r.day_of_month.unwrap_or(1), r.time_of_day.as_deref().unwrap_or("?")),
                        ScheduleType::IntervalDays => format!("Every {} days", r.interval.unwrap_or(1)),
                        ScheduleType::IntervalHours => format!("Every {} hours", r.interval.unwrap_or(1)),
                        ScheduleType::IntervalMinutes => format!("Every {} minutes", r.interval.unwrap_or(1)),
                    };
                    let status = if r.is_active { "ON" } else { "OFF" };
                    out.push_str(&format!(
                        "{}. [{}] \"{}\" | {} | memo: {:.80}\n   id: {}\n",
                        i + 1, status, r.title, schedule, r.memo, r.id
                    ));
                }
                Ok(out)
            }
            "add" => {
                if cfg.routines.len() >= 20 {
                    return Err("Too many routines (max 20). Remove some first.".to_string());
                }

                let title = args["title"]
                    .as_str()
                    .ok_or("Missing 'title' (string)")?
                    .to_string();
                let memo = args["memo"]
                    .as_str()
                    .ok_or("Missing 'memo' (string: the action/message for the AI when triggered)")?
                    .to_string();
                let schedule_type_str = args["schedule_type"]
                    .as_str()
                    .ok_or("Missing 'schedule_type' (string: 'daily', 'weekly', 'monthly', 'interval_days', 'interval_hours', 'interval_minutes')")?;

                let schedule_type = match schedule_type_str.to_lowercase().as_str() {
                    "daily" => ScheduleType::Daily,
                    "weekly" => ScheduleType::Weekly,
                    "monthly" => ScheduleType::Monthly,
                    "interval_days" => ScheduleType::IntervalDays,
                    "interval_hours" => ScheduleType::IntervalHours,
                    "interval_minutes" => ScheduleType::IntervalMinutes,
                    _ => return Err(format!("Invalid schedule_type '{}'. Must be one of: daily, weekly, monthly, interval_days, interval_hours, interval_minutes", schedule_type_str)),
                };

                let time_of_day = args["time_of_day"].as_str().map(|s| s.to_string());
                let day_of_week = args["day_of_week"].as_u64().map(|v| v as u32);
                let day_of_month = args["day_of_month"].as_u64().map(|v| v as u32);
                let interval = args["interval"].as_u64().map(|v| v.max(1) as u32);

                // Validate required fields per schedule type
                match schedule_type {
                    ScheduleType::Weekly if day_of_week.is_none() => {
                        return Err("Weekly schedule requires 'day_of_week' (0=Mon..6=Sun)".to_string());
                    }
                    ScheduleType::Monthly if day_of_month.is_none() => {
                        return Err("Monthly schedule requires 'day_of_month' (1-31)".to_string());
                    }
                    ScheduleType::IntervalDays | ScheduleType::IntervalHours | ScheduleType::IntervalMinutes
                        if interval.is_none() =>
                    {
                        return Err("Interval schedule requires 'interval' (positive integer)".to_string());
                    }
                    _ => {}
                }

                let routine = RoutineDef {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: title.clone(),
                    schedule_type,
                    day_of_week,
                    day_of_month,
                    interval,
                    time_of_day,
                    memo,
                    is_active: true,
                };

                cfg.routines.push(routine);
                cfg.save();

                Ok(format!("Routine '{}' added successfully. It is now active.", title))
            }
            "remove" => {
                let id = args["id"]
                    .as_str()
                    .ok_or("Missing 'id' (string). You MUST call this tool with action='list' first, then copy the exact UUID from the output. Never guess an ID.")?;

                let before = cfg.routines.len();
                let removed_title = cfg.routines.iter().find(|r| r.id == id).map(|r| r.title.clone());
                cfg.routines.retain(|r| r.id != id);

                if cfg.routines.len() == before {
                    return Err(format!("No routine found with id '{}'. Use action='list' to see all routine IDs.", id));
                }

                cfg.save();
                Ok(format!("Routine '{}' (id={}) removed successfully.", removed_title.unwrap_or_default(), id))
            }
            _ => Err(format!("Unknown action '{}'. Use 'add', 'list', or 'remove'.", action)),
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
                            "enum": ["add", "list", "remove"],
                            "description": "The action to perform."
                        },
                        "title": {
                            "type": "string",
                            "description": "[add] Human-readable name for the routine."
                        },
                        "memo": {
                            "type": "string",
                            "description": "[add] The instruction/message that will be sent to you (the AI) when the routine triggers. Write it as a system event prompt."
                        },
                        "schedule_type": {
                            "type": "string",
                            "enum": ["daily", "weekly", "monthly", "interval_days", "interval_hours", "interval_minutes"],
                            "description": "[add] The schedule type."
                        },
                        "time_of_day": {
                            "type": "string",
                            "description": "[add] Time in HH:MM format (for daily/weekly/monthly)."
                        },
                        "day_of_week": {
                            "type": "integer",
                            "description": "[add] Day of week 0=Mon..6=Sun (required for weekly)."
                        },
                        "day_of_month": {
                            "type": "integer",
                            "description": "[add] Day of month 1-31 (required for monthly)."
                        },
                        "interval": {
                            "type": "integer",
                            "description": "[add] Interval amount (required for interval_days/hours/minutes)."
                        },
                        "id": {
                            "type": "string",
                            "description": "[remove] The exact UUID of the routine to remove. You MUST call with action='list' first and copy the ID from the output. NEVER guess or fabricate an ID."
                        }
                    },
                    "required": ["action"]
                }
            }
        })
    }
}
