use crate::ai::skills::Skill;
use crate::interaction::ActionScheduler;
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct ScheduleReminderSkill {
    scheduler: ActionScheduler,
}

impl ScheduleReminderSkill {
    pub fn new(scheduler: ActionScheduler) -> Self {
        Self { scheduler }
    }
}

#[async_trait]
impl Skill for ScheduleReminderSkill {
    fn name(&self) -> &str {
        "schedule_reminder"
    }

    fn description(&self) -> &str {
        "Schedules a proactive reminder or future check-in. Use this when the user asks for a reminder or when you want to follow up on a task later. Safety: Max 5 active reminders, min 1 minute delay."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let minutes = args["minutes"]
            .as_u64()
            .ok_or("Missing or invalid 'minutes' (integer)")? as u32;
        let memo = args["memo"]
            .as_str()
            .ok_or("Missing 'memo' (string)")?
            .to_string();
        tracing::info!("[ReminderSkill] scheduling: {}min | memo: {:.100}", minutes, memo);

        self.scheduler.schedule(minutes, memo)
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
                        "minutes": {
                            "type": "integer",
                            "description": "How many minutes from now to trigger the reminder (min 1)."
                        },
                        "memo": {
                            "type": "string",
                            "description": "What you want to speak to the user when the time comes (e.g., 'Reminder: Meeting in 5 mins')."
                        }
                    },
                    "required": ["minutes", "memo"]
                }
            }
        })
    }
}
