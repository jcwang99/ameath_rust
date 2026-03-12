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
            use base64::{Engine as _, engine::general_purpose::STANDARD};

            // Encode text to base64 to completely avoid PowerShell string escaping and encoding issues
            let title_b64 = STANDARD.encode(title.as_bytes());
            let msg_b64 = STANDARD.encode(message.as_bytes());

            let ps_script = format!(
                "$titleBytes = [System.Convert]::FromBase64String('{}'); \
                 $msgBytes = [System.Convert]::FromBase64String('{}'); \
                 $title = [System.Text.Encoding]::UTF8.GetString($titleBytes); \
                 $message = [System.Text.Encoding]::UTF8.GetString($msgBytes); \
                 [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
                 [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom, ContentType = WindowsRuntime] | Out-Null; \
                 $xmlString = \"<toast><visual><binding template='ToastText02'><text id='1'>$title</text><text id='2'>$message</text></binding></visual></toast>\"; \
                 $toastXml = [Windows.Data.Xml.Dom.XmlDocument]::new(); \
                 $toastXml.LoadXml($xmlString); \
                 $toast = [Windows.UI.Notifications.ToastNotification]::new($toastXml); \
                 [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Ameath AI').Show($toast);",
                title_b64, msg_b64
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
        "Sends a persistent system-level notification (Toast) to the Windows taskbar. You should use your own judgment to decide if a piece of information or a reminder is critical enough to warrant a system-level alert. Use this when the user is away or when you need to provide a hard-to-miss signal."
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
