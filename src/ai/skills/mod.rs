pub trait Skill {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
}

pub struct SkillManager {
    // Placeholder for registered skills
}

impl SkillManager {
    pub fn new() -> Self {
        Self {}
    }
}
