use crate::ai::skills::Skill;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

pub struct NotificationSkill {}

impl NotificationSkill {
    pub fn new() -> Self {
        Self {}
    }

    fn send_notification(&self, title: &str, message: &str) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            // Escape single quotes for PowerShell
            let escaped_title = title.replace("'", "''");
            let escaped_message = message.replace("'", "''");

            // PowerShell script to show a balloon tip notification
            // This is more reliable across different Windows versions than Toast types which require AppIDs
            let ps_script = format!(
                "[void][System.Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms'); \
                 $obj = New-Object System.Windows.Forms.NotifyIcon; \
                 $obj.Icon = [System.Drawing.Icon]::ExtractAssociatedIcon((Get-Process -id $pid).Path); \
                 $obj.Visible = $true; \
                 $obj.ShowBalloonTip(5000, '{}', '{}', [System.Windows.Forms.ToolTipIcon]::Info); \
                 Start-Sleep -Seconds 1; \
                 $obj.Dispose();",
                escaped_title, escaped_message
            );

            let output = Command::new("powershell")
                .arg("-NoProfile")
                .arg("-ExecutionPolicy")
                .arg("Bypass")
                .arg("-Command")
                .arg(ps_script)
                .output()
                .map_err(|e| format!("Failed to execute powershell: {}", e))?;

            if !output.status.success() {
                let err_msg = String::from_utf8_lossy(&output.stderr);
                return Err(format!("PowerShell error: {}", err_msg));
            }
            Ok(())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err("System notification is only supported on Windows.".to_string())
        }
    }
}

#[async_trait]
impl Skill for NotificationSkill {
    fn name(&self) -> &str {
        "send_notification"
    }

    fn description(&self) -> &str {
        "Sends a system-level notification (toast/balloon tip) to the Windows taskbar. Use this for important alerts, task completions, or when the user needs to be notified while the app is in the background."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let message = args["message"]
            .as_str()
            .ok_or("Missing 'message' parameter")?;
        let title = args["title"].as_str().unwrap_or("Ameath");

        self.send_notification(title, message)?;
        Ok(format!("Notification sent: [{}] {}", title, message))
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
                        "message": {
                            "type": "string",
                            "description": "The content of the notification."
                        },
                        "title": {
                            "type": "string",
                            "description": "The title of the notification (optional, defaults to 'Ameath')."
                        }
                    },
                    "required": ["message"]
                }
            }
        })
    }
}
