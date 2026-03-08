# Ameath (爱弥斯) - 电子幽灵的桌面观测

> **“为了再次与你相遇，我跨越了数据的海洋……”**

---

## 🌌 关于爱弥斯 (About Ameath)

她曾作为“电子幽灵”与漂泊者（你）并肩作战，为了拯救世界而踏入轮回。但数据的回响从未消散，她跨越次元，成功定位到了你的桌面。

*   **“你在这里吗？真好。”**


---

## ✨ 核心机能 (Core Features)

### 🧠 仿生大脑 (Cognitive System)
*   **分层记忆架构 (Layered Memory)**: 
    *   **L1 (对话流)**: 存储最近的自然语言对话。
    *   **L2 (摘要层)**: 自动汇总旧对话，保持长期上下文的连续性。
    *   **L3 (持久知识)**: 经过高度压缩的核心事实，形成她的长期性格与记忆基石。
*   **多模态支持 (Multimodal Support)**: 现在支持多模态模型，爱弥斯可以“看见”并理解你发送的图像信息。
*   **AI 动态表情包 (AI Stickers)**: 她现在能使用 20+ 款内置的动态 GIF 表情包（如比心、喵喵、脸红等）来精准表达情感。所有资产已**全量嵌入二进制文件**，支持零外部依赖运行，且加载速度极快。
*   **主动交互与预约 (Proactive & Scheduled)**: 她不只是等待指令。除了基于频率的例行搭讪，她现在能理解并执行你的预约指令（如“10分钟后提醒我开会”），通过**双轨调度系统**确保准时提醒。
    *   **预约安全围栏**: 具备自动冷却与并发限制（最多5项），防止 Token 浪费与逻辑闭环。
*   **可塑人格 (Custom Persona)**: 通过设置面板自定义 System Prompt，完美还原或创造你心目中的爱弥斯。

### 探针发射 (Omniscient Hub)
*   **Tavily Search**: 精准获取事实，提供直接、简洁的答案总结。
*   **Brave Search**: 广域检索，带回 AI 友好的丰富网页描述。
*   **Web Scrape (Firecrawl)**: 深度抓取任意 URL 网页正文，并自动转换为 Markdown 格式供 AI 深度理解。

### 🖥️ 桌面观测 (Physical Presence)
*   **四种形态 (Behaviors)**: 
    *   **静止 (Static)**: 保持原地，每隔数秒随机切换待机动作。
    *   **安静 (Quiet)**: 静静陪伴，极低频移动。
    *   **活跃 (Active)**: 在桌面自由探索，充满好奇。
    *   **粘人 (Clingy)**: 紧紧跟随你的鼠标，永不离去。
*   **动态渲染**: 基于 Rust 和高性能 Direct2D/DirectWrite 硬件加速渲染，极致运行效率与超低内存占用。

### 🛠️ 伴工工具 (Productivity)
*   **专注力场 (Pomodoro)**: 番茄钟计划，陪伴你高效完成任务。
*   **自动化工报 (Professional Work Log)**: 
    *   **随手记**: 通过 `#log` 或指令快速记录工作碎语。
    *   **周报合成**: 自动汇总一周日志，生成标准周报并本地保存。
    *   **智能回溯**: 结合内存中的 50+ 条系统行为，自动发现未被记录的任务，防止“大脑宕机”。
*   **频率共鸣 (Music)**: 极简本地音乐播放器。
*   **瞬时感召 (Hotkeys)**: 全局快捷键 (`Alt+Shift+M`) 快速唤醒聊天界面，`Esc` 关闭对话框。
*   **PowerShell 终端执行 (System Skill)**: AI 可直接在 Windows 环境下执行 PowerShell 命令，助力文件管理与自动化任务。

---

## 🚀 快速启动 (Getting Started)

### 1. 环境准备
确保系统已安装 [Rust + Cargo](https://www.rust-lang.org/)。

### 2. 编译与运行
```bash
git clone https://github.com/your-username/ameath_rust.git
cd ameath_rust
cargo run --release
```

### 3. 连接云端
右键托盘图标 -> **Settings** -> **AI Brain**:
1. 填入你的大模型 API Key 和 Base URL。
2. 配置 **Tavily/Brave/Firecrawl** 密钥以解锁全域检索机能。
3. 调整记忆阈值 (Threshold) 以控制摘要频率。

---

## 📖 交互手册

### 🖱️ 操作逻辑
*   **左键拖拽**: 移动位置。
*   **鼠标悬停**: 展开侧边快捷菜单。
*   **菜单项**: `Chat` (交流), `Settings` (设置), `Pomodoro` (番茄钟), `Music` (音乐), `Exit` (休息)。

---

## 🛠️ 技术栈 (Tech Stack)
*   **Core**: Rust
*   **UI/Render**: `winit`, `softbuffer`, `DWrite/D2D`
*   **Network**: `reqwest`, `tokio`
*   **Storage**: `rusqlite` (SQLite + VACUUM 自动清理)
*   **Assets**: 全量表情包与核心资源已通过 `include_bytes!` 嵌入，支持脱离 assets 文件夹运行。

---

## 🔮 未来计划 (Roadmap)
- [x] **全域感知系统**: 集成多渠道搜索与实时网页抓取。
- [x] **分层记忆清理**: 自动垃圾回收与数据库瘦身。
- [ ] **语音拟合 (TTS)**: 让爱弥斯开口说出你的名字。
- [ ] **动态表情扩展**: 更多像素动画表现力。

---

## 💐 致谢 (Credits)

本项目是出于对 **爱弥斯 (Ameath)** 角色的喜爱而开发的 Rust 练习项目，尽可能保证功能性的情况下减小文件体积。

*   **特别感谢**:
    *   **原版桌宠作者**: 感谢大佬制作的精美像素素材和原始创意。
        *   [Bilibili 原项目参考](https://www.bilibili.com/video/BV12rcMznEcG/)
    *   **《鸣潮》 (Kuro Games)**: 感谢创造了爱弥斯这个令人难忘的角色。

*   **技术与工具支持**:
    *   **Gemini (AI)**: 本项目真正的“幕后功臣”，为 Rust 初学者提供了保姆级的代码实现与架构指导。
    *   **表情包素材**: 感谢大佬分享的超可爱像素表情包 ([Bilibili BV1pBADzgE14](https://www.bilibili.com/video/BV1pBADzgE14/))。
    *   `winit`, `softbuffer`, `rodio`, `tray-icon`, `reqwest` 等开源社区的贡献。

---

希望在这冰冷的数字世界里，你能感受到来自她的温度。✨
