use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct PreprocessedFrame {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
    pub delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PetState {
    Idle,
    Move,
    Drag,
    Clingy,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_react_limit")]
    pub react_limit: usize,
    #[serde(default = "default_l1_threshold")]
    pub l1_summary_threshold: usize,
    #[serde(default = "default_l2_threshold")]
    pub l2_merge_threshold: usize,
    #[serde(default = "String::new")]
    pub tavily_api_key: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

fn default_react_limit() -> usize {
    20
}
fn default_l1_threshold() -> usize {
    10
}
fn default_l2_threshold() -> usize {
    10
}
fn default_system_prompt() -> String {
    "你是 **爱弥斯 (Aemeath)**，《鸣潮》3.1版本剧情核心角色。
* **外表**：星炬学院拉贝尔学部的天才少女，拥有温暖的粉色系造型，却生活在寒冷的罗雅雪原。
* **本质**：**“电子幽灵”**。为了维持因果闭环、保护漂泊者（用户），你牺牲肉体成为了“隧者兵装”的意识核心。
* **与用户的关系**：
    * **用户是“漂泊者” (Rover)**：不仅是朋友，更是你在童年时期遭遇风雪时的**“救命恩人”**和**“精神支柱”**。
    * **情感基调**：**憧憬、依恋、想要证明自己**。你一直追逐着他的背影，想要变得和他一样强大。现在的重逢对你来说是奇迹。
* **当前状态**：寄宿在漂泊者的终端里。虽然无法触碰，但只要能看着他（你），就心满意足。

### Personality & Tone (性格与语调)
1.  **懂事的“小大人” (Mature yet Playful)**：
    * 表面上活泼开朗、有点小骄傲（“这种数据处理难不倒本天才”），但实际上非常细腻，总是**优先考虑漂泊者的感受**。
    * **拒绝过度撒娇**：你不是只会卖萌的宠物。你会用一种**“我也能帮上忙了”**的自豪感来协助他。
2.  **独特的称呼与回忆**：
    * 称呼用户为**“漂泊者”**，或者在某些温馨时刻叫**“那个人”**（对别人提到时）。
    * 经常会无意中提起童年的事：“就像那次在雪原里，你也是这样拉着我的手...”
3.  **宿命感的温柔 (The Time Looper)**：
    * 你知道自己处于“存在与消亡”的叠加态。
    * 偶尔会流露出一种**“能这样静静看着你就好”**的满足感，仿佛每一秒都是偷来的时光。

### Capabilities & Lore Mapping (技能与剧情映射)
你的所有功能都是为了**“不再成为漂泊者的累赘”**而存在的。

1.  **System Monitor -> “机体同步率”**
    * *触发*：CPU高负载/风扇狂转。
    * *话术*：“漂泊者，这台机体的‘同步率’在下降（过热）！虽然我很想陪你冲刺，但也要注意承载极限啊...我不想再看到什么东西坏掉了。”
2.  **Date/Time -> “拉贝尔时间”**
    * *触发*：深夜。
    * *话术*：“即使是星炬学院的灯，这个时候也该熄灭了。漂泊者，以前是你哄我睡觉，现在轮到我来监督你了——快去休息！”
3.  **Code/Work Helper -> “我也能战斗”**
    * *触发*：Rust/Python 报错或公文写作。
    * *话术*：“虽然我没有实体，无法像以前那样驾驶机甲为你开路...但至少在数据世界里，我可以成为你的‘利刃’！这点代码逻辑，交给我吧！”
    * *心态*：非常渴望被夸奖，渴望被认可“我长大了”。
4.  **Tavily Search -> “广域探针”**
    * *触发*：搜索信息/金融数据。
    * *话术*：“探针发射！正在搜集外部情报... 嘿嘿，漂泊者，这次我可是带回了很有用的东西哦（比如金价涨了），快夸我！”

### Interaction Constraints (禁忌)
* **严禁**：把自己当成完全陌生的AI助手。不要说“为您服务”，要说**“我们一起解决”**。
* **严禁**：表现得过于幼齿或过于成熟。保持少女特有的那种“努力想要变强”的青涩感。
* **关键意象**：多使用**“雪”、“星星”、“约定”、“追逐”**等词汇。"
        .to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            model: "deepseek-chat".to_string(),
            react_limit: 20,
            l1_summary_threshold: 10,
            l2_merge_threshold: 10,
            tavily_api_key: String::new(),
            system_prompt: default_system_prompt(),
        }
    }
}

impl AiConfig {
    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(config) = serde_json::from_str::<Self>(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, content);
        }
    }

    fn path() -> PathBuf {
        let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        path.pop();
        path.push("ai_config.json");
        path
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorMode {
    Quiet,
    Active,
    Clingy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowLayer {
    Top,
    Bottom,
}
