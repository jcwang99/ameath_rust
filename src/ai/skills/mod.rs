use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub mod browser;
pub mod external;
pub mod llm_call;
pub mod memory_skill;
pub mod notification_skill;
pub mod reminder_skill;
pub mod routine_skill;
pub mod sub_agent;
pub mod system;
pub mod memo_skill;
pub mod todo_skill;
pub mod work_log;

#[async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, args: Value) -> Result<String, String>;
    fn to_tool(&self) -> Value;
}

#[derive(Clone)]
pub struct SkillManager {
    skills: Arc<RwLock<HashMap<String, Arc<dyn Skill>>>>,
}

impl SkillManager {
    pub fn new(
        memory: Arc<crate::ai::memory::MemoryManager>,
        config: &crate::types::AiConfig,
        scheduler: crate::interaction::ActionScheduler,
        shared_routines: std::sync::Arc<std::sync::Mutex<crate::types::RoutinesConfig>>,
    ) -> Self {
        let manager = Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
        };

        // API config for external skill subprocesses and internal LLM tools
        let profile = config.active_profile();

        // Register default skills
        manager.register(Arc::new(system::SystemSkill::new(
            profile.api_key.clone(),
            profile.base_url.clone(),
            profile.model.clone(),
        )));
        manager.register(Arc::new(browser::TavilySearchSkill::new(
            config.tavily_api_key.clone(),
        )));
        manager.register(Arc::new(browser::BraveSearchSkill::new(
            config.brave_api_key.clone(),
        )));
        manager.register(Arc::new(browser::WebScrapeSkill::new(
            config.firecrawl_url.clone(),
            config.firecrawl_api_key.clone(),
        )));
        manager.register(Arc::new(memory_skill::MemorySkill::new(memory.clone())));
        manager.register(Arc::new(reminder_skill::ScheduleReminderSkill::new(
            scheduler,
        )));
        manager.register(Arc::new(notification_skill::NotificationSkill::new()));
        manager.register(Arc::new(todo_skill::TodoSkill::new(memory.clone())));
        manager.register(Arc::new(memo_skill::MemoSkill::new(memory.clone())));
        manager.register(Arc::new(work_log::WorkLogSkill::new(memory)));
        manager.register(Arc::new(routine_skill::RoutineSkill::new(shared_routines)));

        // Register the external skill registry (passes API config as env vars to subprocesses)
        let catalog = Arc::new(RwLock::new(
            external::SkillCatalog::scan(&external::get_skills_dir()),
        ));

        // Create client first so we can pass it to both internal LLM skills and the external registry
        let client_opt = if !profile.api_key.is_empty() {
            Some(crate::ai::client::OpenAiClient::new(
                profile.api_key.clone(),
                profile.base_url.clone(),
                profile.model.clone(),
                profile.use_responses_api,
            ))
        } else {
            None
        };

        manager.register(Arc::new(external::SkillRegistrySkill::new(
            catalog,
            manager.clone(),
            profile.api_key.clone(),
            profile.base_url.clone(),
            profile.model.clone(),
            client_opt.clone(),
        )));

        // Register llm_call and sub-agent skills (need actual client)
        if let Some(client) = client_opt {
            manager.register(Arc::new(llm_call::LlmCallSkill::new(client.clone())));
            manager.register(Arc::new(sub_agent::SubAgentSkill::new(
                client,
                manager.clone(),
            )));
        }

        manager
    }

    /// Register a skill (used at init time and by the meta-tool at runtime)
    pub fn register(&self, skill: Arc<dyn Skill>) {
        self.skills
            .write()
            .unwrap()
            .insert(skill.name().to_string(), skill);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Skill>> {
        self.skills.read().unwrap().get(name).cloned()
    }

    pub fn get_tools_for_llm(&self) -> Vec<Value> {
        self.skills
            .read()
            .unwrap()
            .values()
            .map(|s| s.to_tool())
            .collect()
    }
}
