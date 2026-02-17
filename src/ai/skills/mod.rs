use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub mod browser;
pub mod memory_skill;
pub mod reminder_skill;
pub mod system;

#[async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, args: Value) -> Result<String, String>;
    fn to_tool(&self) -> Value;
}

#[derive(Clone)]
pub struct SkillManager {
    skills: HashMap<String, Arc<dyn Skill>>,
}

impl SkillManager {
    pub fn new(
        memory: Arc<crate::ai::memory::MemoryManager>,
        config: &crate::types::AiConfig,
        scheduler: crate::interaction::ActionScheduler,
    ) -> Self {
        let mut manager = Self {
            skills: HashMap::new(),
        };

        // Register default skills
        manager.register(Arc::new(system::SystemSkill::new()));
        // Register independent browser skills
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
        manager.register(Arc::new(memory_skill::MemorySkill::new(memory)));
        manager.register(Arc::new(reminder_skill::ScheduleReminderSkill::new(
            scheduler,
        )));

        manager
    }

    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        self.skills.insert(skill.name().to_string(), skill);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Skill>> {
        self.skills.get(name).cloned()
    }

    pub fn get_tools_for_llm(&self) -> Vec<Value> {
        self.skills.values().map(|s| s.to_tool()).collect()
    }
}
