use crate::ai::skills::Skill;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

pub struct SystemSkill;

impl SystemSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for SystemSkill {
    fn name(&self) -> &str {
        "execute_command"
    }

    fn description(&self) -> &str {
        "Executes a PowerShell command on Windows. Use this for file operations, system checks, or running scripts."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let command_str = args["command"]
            .as_str()
            .ok_or_else(|| "Missing 'command' argument".to_string())?;

        let output = if cfg!(target_os = "windows") {
            Command::new("powershell")
                .arg("-Command")
                .arg(command_str)
                .output()
                .map_err(|e| format!("Failed to execute powershell: {}", e))?
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(command_str)
                .output()
                .map_err(|e| format!("Failed to execute sh: {}", e))?
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            Err(format!(
                "Command failed with status {}:\n{}",
                output.status, stderr
            ))
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
                        "command": {
                            "type": "string",
                            "description": "The PowerShell/Shell command to execute."
                        }
                    },
                    "required": ["command"]
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_execute_powershell() {
        let skill = SystemSkill::new();
        let args = json!({
            "command": "echo 'Hello from PowerShell'"
        });
        let result = skill.execute(args).await.unwrap();
        assert!(result.contains("Hello from PowerShell"));
    }

    #[tokio::test]
    async fn test_execute_powershell_dir() {
        let skill = SystemSkill::new();
        let args = json!({
            "command": "dir"
        });
        let result = skill.execute(args).await.unwrap();
        // Check for common file attributes in dir output
        assert!(
            result.contains("Directory")
                || result.contains("Mode")
                || result.contains("LastWriteTime")
        );
    }
}
