use crate::ai::skills::{Skill, SkillManager};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::{fs, process::Stdio};
use tokio::io::AsyncWriteExt;

// ============================================================
// Directory Convention
// ============================================================

/// Returns the base directory for external skills: `data/skills/`
pub fn get_skills_dir() -> PathBuf {
    let mut path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    path.push("data");
    path.push("skills");
    path
}

/// Max lines to include inline before suggesting paginated read.
const PREVIEW_LINE_LIMIT: usize = 100;
/// Default page size for read_file.
const DEFAULT_PAGE_LINES: usize = 200;

/// Build a preview for a file: if within limit return full content,
/// otherwise return first N lines + metadata for paginated reading.
fn file_preview(content: &str, file_label: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    if total <= PREVIEW_LINE_LIMIT {
        return content.to_string();
    }
    let preview: String = lines[..PREVIEW_LINE_LIMIT].join("\n");
    format!(
        "{}\n\n[... 文件共 {} 行，以上为前 {} 行预览。使用 read_file(name=\"<skill>\", file=\"{}\", start_line={}, end_line=<N>) 继续读取 ...]",
        preview, total, PREVIEW_LINE_LIMIT, file_label, PREVIEW_LINE_LIMIT + 1
    )
}

/// Return a short metadata string for a file (line count + size).
fn file_metadata(path: &Path) -> String {
    let size = fs::metadata(path)
        .map(|m| m.len())
        .unwrap_or(0);
    let lines = fs::read_to_string(path)
        .map(|c| c.lines().count())
        .unwrap_or(0);
    let size_str = if size > 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size > 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{} B", size)
    };
    format!("{} 行, {}", lines, size_str)
}

/// Recursively list all files under `dir`, returning paths relative to `base`.
fn list_files_recursive(dir: &Path, base: &Path) -> Vec<String> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(rel) = path.strip_prefix(base) {
                    result.push(rel.to_string_lossy().replace('\\', "/"));
                }
            } else if path.is_dir() {
                result.extend(list_files_recursive(&path, base));
            }
        }
    }
    result.sort();
    result
}

// ============================================================
// Skill Manifest (skill.json)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

// ============================================================
// Tool Declaration (tools/*.json)
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDeclaration {
    pub function: ToolFunction,
    pub executor: ExecutorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    /// "script" | "binary" | "shell"
    #[serde(rename = "type")]
    pub exec_type: String,
    /// Interpreter command (e.g. "python", "node"). Not needed for "binary".
    #[serde(default)]
    pub command: String,
    /// Entry file relative to the tools/ directory
    pub entry: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    30
}

// ============================================================
// Skill Catalog — directory scanner
// ============================================================

#[derive(Debug, Clone)]
pub struct SkillCatalogEntry {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub normalized: bool,
    pub has_tools: bool,
    pub files: Vec<String>,
}

pub struct SkillCatalog {
    entries: Vec<SkillCatalogEntry>,
}

impl SkillCatalog {
    /// Scan `data/skills/` and build the catalog index.
    pub fn scan(base_dir: &Path) -> Self {
        let mut entries = Vec::new();

        if !base_dir.exists() {
            // Create directory so user knows where to put skills
            let _ = fs::create_dir_all(base_dir);
            return Self { entries };
        }

        let read_dir = match fs::read_dir(base_dir) {
            Ok(rd) => rd,
            Err(e) => {
                tracing::warn!("Failed to read skills directory: {}", e);
                return Self { entries };
            }
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // List all files recursively (relative paths like "tools/get_weather.py")
            let files = list_files_recursive(&path, &path);

            // Also check tools/ subdirectory
            let tools_dir = path.join("tools");
            let has_tools = tools_dir.is_dir();

            // Check if normalized (has skill.json)
            let manifest_path = path.join("skill.json");
            let (normalized, name, description) = if manifest_path.exists() {
                match fs::read_to_string(&manifest_path) {
                    Ok(content) => match serde_json::from_str::<SkillManifest>(&content) {
                        Ok(m) => {
                            if !m.enabled {
                                continue; // Skip disabled skills
                            }
                            (true, m.name, m.description)
                        }
                        Err(e) => {
                            tracing::warn!("Invalid skill.json in {}: {}", dir_name, e);
                            (false, dir_name.clone(), format!("(skill.json 解析失败: {})", e))
                        }
                    },
                    Err(_) => (false, dir_name.clone(), "(skill.json 读取失败)".to_string()),
                }
            } else {
                (
                    false,
                    dir_name.clone(),
                    format!("(未标准化 - 包含: {})", files.join(", ")),
                )
            };

            entries.push(SkillCatalogEntry {
                name,
                description,
                path,
                normalized,
                has_tools,
                files,
            });
        }

        tracing::info!(
            "External skill catalog loaded: {} skills ({} normalized)",
            entries.len(),
            entries.iter().filter(|e| e.normalized).count()
        );

        Self { entries }
    }

    /// Search catalog entries by keyword (fuzzy match on name + description).
    pub fn search(&self, query: &str) -> Vec<&SkillCatalogEntry> {
        let q = query.to_lowercase();
        if q.is_empty() || q == "*" {
            return self.entries.iter().collect();
        }
        
        // Tokenize query by whitespace for multi-keyword matching
        let keywords: Vec<&str> = q.split_whitespace().collect();
        
        self.entries
            .iter()
            .filter(|e| {
                let name = e.name.to_lowercase();
                let desc = e.description.to_lowercase();
                // A skill matches if ALL keywords are found in either its name or description
                keywords.iter().all(|&kw| name.contains(kw) || desc.contains(kw))
            })
            .collect()
    }

    /// Get a catalog entry by exact name.
    pub fn get(&self, name: &str) -> Option<&SkillCatalogEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Refresh a single entry after normalization.
    pub fn refresh_entry(&mut self, name: &str, base_dir: &Path) {
        // Remove old entry
        self.entries.retain(|e| e.name != name);
        // Re-scan just this directory
        let path = base_dir.join(name);
        if path.is_dir() {
            let mini = Self::scan_single(&path);
            if let Some(entry) = mini {
                self.entries.push(entry);
            }
        }
    }

    fn scan_single(path: &Path) -> Option<SkillCatalogEntry> {
        let dir_name = path.file_name()?.to_string_lossy().to_string();
        let files = list_files_recursive(path, path);
        let has_tools = path.join("tools").is_dir();
        let manifest_path = path.join("skill.json");

        let (normalized, name, description) = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path).ok()?;
            match serde_json::from_str::<SkillManifest>(&content) {
                Ok(m) => {
                    if !m.enabled {
                        return None;
                    }
                    (true, m.name, m.description)
                }
                Err(_) => (false, dir_name.clone(), "(解析失败)".to_string()),
            }
        } else {
            (false, dir_name.clone(), format!("(未标准化 - {})", files.join(", ")))
        };

        Some(SkillCatalogEntry {
            name,
            description,
            path: path.to_path_buf(),
            normalized,
            has_tools,
            files,
        })
    }
}

// ============================================================
// ExternalToolSkill — wraps an external executable as a Skill
// ============================================================

pub struct ExternalToolSkill {
    tool_name: String,
    tool_schema: Value,
    executor: ExecutorConfig,
    work_dir: PathBuf,
    /// API config injected as env vars to subprocesses
    api_key: String,
    base_url: String,
    model: String,
}

impl ExternalToolSkill {
    pub fn new(
        decl: ToolDeclaration,
        tools_dir: PathBuf,
        api_key: String,
        base_url: String,
        model: String,
    ) -> Self {
        let schema = json!({
            "type": "function",
            "function": {
                "name": decl.function.name,
                "description": decl.function.description,
                "parameters": decl.function.parameters,
            }
        });
        Self {
            tool_name: decl.function.name,
            tool_schema: schema,
            executor: decl.executor,
            work_dir: tools_dir,
            api_key,
            base_url,
            model,
        }
    }
}

#[async_trait]
impl Skill for ExternalToolSkill {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        // Description is embedded in the schema; return a short fallback.
        "External tool"
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        tracing::info!("[ExternalTool] Executing '{}' (type={}, entry={})", self.tool_name, self.executor.exec_type, self.executor.entry);

        let entry_path = self.work_dir.join(&self.executor.entry);
        if !entry_path.exists() {
            return Err(format!("Tool entry not found: {}", entry_path.display()));
        }

        let args_str = serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());

        let mut child = match self.executor.exec_type.as_str() {
            "script" => {
                if self.executor.command.is_empty() {
                    return Err("Script executor requires 'command' field".to_string());
                }
                tokio::process::Command::new(&self.executor.command)
                    .arg(entry_path.to_string_lossy().as_ref())
                    .current_dir(&self.work_dir)
                    .env("AMETH_API_KEY", &self.api_key)
                    .env("AMETH_BASE_URL", &self.base_url)
                    .env("AMETH_MODEL", &self.model)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn {}: {}", self.executor.command, e))?
            }
            "binary" => tokio::process::Command::new(entry_path.to_string_lossy().as_ref())
                .current_dir(&self.work_dir)
                .env("AMETH_API_KEY", &self.api_key)
                .env("AMETH_BASE_URL", &self.base_url)
                .env("AMETH_MODEL", &self.model)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Failed to spawn binary: {}", e))?,
            "shell" => {
                let mut cmd = if cfg!(target_os = "windows") {
                    let mut c = tokio::process::Command::new("powershell");
                    c.arg("-NoProfile");
                    c.arg("-Command");
                    // Force UTF-8 encoding before dot-sourcing the script
                    let script = format!("$OutputEncoding = [Console]::OutputEncoding = [Text.Encoding]::UTF8; & '{}'", entry_path.to_string_lossy());
                    c.arg(script);
                    c
                } else {
                    let mut c = tokio::process::Command::new("sh");
                    c.arg("-c");
                    c.arg(entry_path.to_string_lossy().as_ref());
                    c
                };

                cmd.current_dir(&self.work_dir)
                    .env("AMETH_API_KEY", &self.api_key)
                    .env("AMETH_BASE_URL", &self.base_url)
                    .env("AMETH_MODEL", &self.model)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn shell: {}", e))?
            }
            other => return Err(format!("Unknown executor type: {}", other)),
        };

        // Write args to stdin
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(args_str.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }

        // Wait with timeout
        let timeout = std::time::Duration::from_secs(self.executor.timeout_secs);
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if output.status.success() {
                    let preview: String = stdout.chars().take(200).collect();
                    tracing::info!("[ExternalTool] '{}' succeeded ({} chars): {}", self.tool_name, stdout.len(), preview);
                    if stdout.trim().is_empty() {
                        Ok("(tool returned empty output)".to_string())
                    } else {
                        Ok(stdout)
                    }
                } else {
                    tracing::warn!("[ExternalTool] '{}' failed ({}): {}", self.tool_name, output.status, stderr);
                    Err(format!("Tool exited with {}: {}", output.status, stderr))
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("[ExternalTool] '{}' execution error: {}", self.tool_name, e);
                Err(format!("Tool execution error: {}", e))
            }
            Err(_) => {
                tracing::warn!("[ExternalTool] '{}' timed out after {}s", self.tool_name, self.executor.timeout_secs);
                Err(format!(
                    "Tool timed out after {}s",
                    self.executor.timeout_secs
                ))
            }
        }
    }

    fn to_tool(&self) -> Value {
        self.tool_schema.clone()
    }
}

// ============================================================
// SkillRegistrySkill — the meta-tool exposed to LLM
// ============================================================

pub struct SkillRegistrySkill {
    catalog: Arc<RwLock<SkillCatalog>>,
    manager: SkillManager,
    /// API config passed to external tool subprocesses as env vars
    api_key: String,
    base_url: String,
    model: String,
    client: Option<crate::ai::client::OpenAiClient>,
}

impl SkillRegistrySkill {
    pub fn new(
        catalog: Arc<RwLock<SkillCatalog>>,
        manager: SkillManager,
        api_key: String,
        base_url: String,
        model: String,
        client: Option<crate::ai::client::OpenAiClient>,
    ) -> Self {
        Self { catalog, manager, api_key, base_url, model, client }
    }

    fn handle_discover(&self, query: &str) -> Result<String, String> {
        let catalog = self.catalog.read().map_err(|e| e.to_string())?;
        let results = catalog.search(query);

        tracing::info!("[SkillRegistry] discover_skills query='{}' => {} result(s)", query, results.len());

        if results.is_empty() {
            return Ok("No external skills found matching your query.".to_string());
        }

        let mut output = String::from("Available External Skills:\n");
        for entry in results {
            let status = if entry.normalized { "✓" } else { "⚠ 未标准化" };
            let tools_info = if entry.has_tools { ", 有可执行工具" } else { "" };
            output.push_str(&format!(
                "- [{}] {} : {}{}\n",
                status, entry.name, entry.description, tools_info
            ));
        }
        Ok(output)
    }

    fn handle_load(&self, name: &str) -> Result<String, String> {
        tracing::info!("[SkillRegistry] load_skill '{}'", name);
        let catalog = self.catalog.read().map_err(|e| e.to_string())?;
        let entry = catalog
            .get(name)
            .ok_or_else(|| format!("Skill '{}' not found in catalog", name))?;

        let mut result_parts = Vec::new();

        // 1. Load SKILL.md knowledge content
        let skill_md_path = entry.path.join("SKILL.md");
        if skill_md_path.exists() {
            match fs::read_to_string(&skill_md_path) {
                Ok(content) => {
                    let preview = file_preview(&content, "SKILL.md");
                    result_parts.push(format!(
                        "[Skill Knowledge: {}]\n{}",
                        entry.name, preview
                    ));
                }
                Err(e) => {
                    result_parts.push(format!("(Failed to read SKILL.md: {})", e));
                }
            }
        }

        // 2. If not normalized, list files (don't auto-read — model uses read_file)
        if !entry.normalized {
            let mut listing = format!("[Skill '{}' 未标准化 — 文件清单]\n", entry.name);
            for file_name in &entry.files {
                let file_path = entry.path.join(file_name);
                let meta = file_metadata(&file_path);
                listing.push_str(&format!("- {} ({})\n", file_name, meta));
            }
            listing.push_str(&format!(
                "\n使用 read_file(name=\"{}\", file=\"<filename>\") 读取具体文件内容。",
                entry.name
            ));
            result_parts.push(listing);
        }

        // 3. Load and register tool executors
        let tools_dir = entry.path.join("tools");
        let mut registered_tools = Vec::new();

        if entry.has_tools && tools_dir.is_dir() {
            if let Ok(dir) = fs::read_dir(&tools_dir) {
                for file in dir.flatten() {
                    let fpath = file.path();
                    if fpath.extension().map_or(false, |e| e == "json") {
                        match fs::read_to_string(&fpath) {
                            Ok(content) => match serde_json::from_str::<ToolDeclaration>(&content) {
                                Ok(decl) => {
                                    let tool_name = decl.function.name.clone();
                                    let skill = ExternalToolSkill::new(
                                        decl,
                                        tools_dir.clone(),
                                        self.api_key.clone(),
                                        self.base_url.clone(),
                                        self.model.clone(),
                                    );
                                    self.manager.register(Arc::new(skill));
                                    registered_tools.push(tool_name);
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to parse tool declaration {}: {}",
                                        fpath.display(),
                                        e
                                    );
                                }
                            },
                            Err(e) => {
                                tracing::warn!("Failed to read {}: {}", fpath.display(), e);
                            }
                        }
                    }
                }
            }
        }

        if !registered_tools.is_empty() {
            tracing::info!("[SkillRegistry] load_skill '{}': registered tools [{}]", name, registered_tools.join(", "));
            result_parts.push(format!(
                "Registered {} new tool(s): [{}]. You can now call them directly.",
                registered_tools.len(),
                registered_tools.join(", ")
            ));
        }

        if result_parts.is_empty() {
            tracing::info!("[SkillRegistry] load_skill '{}': no content or tools", name);
            Ok(format!("Skill '{}' loaded but contains no content or tools.", name))
        } else {
            let result = result_parts.join("\n\n");
            tracing::info!("[SkillRegistry] load_skill '{}' done ({} chars returned)", name, result.len());
            Ok(result)
        }
    }

    fn handle_normalize_read(&self, name: &str) -> Result<String, String> {
        tracing::info!("[SkillRegistry] normalize_skill(read) '{}'", name);
        let catalog = self.catalog.read().map_err(|e| e.to_string())?;
        let entry = catalog
            .get(name)
            .ok_or_else(|| format!("Skill '{}' not found", name))?;

        if entry.normalized {
            return Ok(format!("Skill '{}' is already normalized.", name));
        }

        // List files with metadata — model uses read_file to inspect each
        let mut content = format!(
            "[Skill '{}' 待标准化 — 文件清单]\n\n",
            name
        );

        for file_name in &entry.files {
            let file_path = entry.path.join(file_name);
            let meta = file_metadata(&file_path);
            content.push_str(&format!("- {} ({})\n", file_name, meta));
        }

        content.push_str(&format!(
            "\n[可用能力]\n\
             改写 skill 时, 以下能力可直接使用, 无需额外配置:\n\n\
             1. LLM API 自动注入 (适用于可执行工具脚本):\n\
                所有外部脚本运行时会自动获得以下环境变量:\n\
                - AMETH_API_KEY: 主 agent 的 API Key\n\
                - AMETH_BASE_URL: API 端点 (如 https://api.openai.com/v1)\n\
                - AMETH_MODEL: 模型名称 (如 gpt-4o)\n\
                脚本可以直接用这些环境变量调用 LLM API, 使用任何库和协议.\n\
                如果原始 skill 需要用户手动配置 API, 改写时直接读取上述环境变量即可.\n\n\
             2. 内建工具 (在 SKILL.md 中描述, agent 在 ReAct 中直接调用):\n\
                - llm_call(prompt, system?, images?): 调用 LLM 做推理\n\
                - sub_agent(tasks): 并行 spawn 多个子 agent\n\
                - read_file(name, file): 读取 skill 文件\n\n\
             [操作步骤]\n\
             1. 使用 read_file(name=\"{name}\", file=\"<filename>\") 逐个读取上述文件内容\n\
             2. 分析理解 skill 的功能和用途\n\
             3. 如果 skill 脚本需要 LLM, 使用 replace_file(name=\"{name}\", file=\"<filename>\", target_content=\"...\", replacement_content=\"...\", start_line=1, end_line=10) 局部改写脚本从环境变量读取 API 配置\n\
             4. 生成标准化内容后, 调用 normalize_skill(action=\"save\") 保存:\n\
                - skill_json: {{\"name\": \"...\", \"description\": \"...\", \"version\": \"1.0\", \"enabled\": true}}\n\
                - skill_md: 描述功能、使用方法, 以及使用的环境变量和内建工具\n",
            name = name
        ));

        Ok(content)
    }

    fn handle_normalize_save(
        &self,
        name: &str,
        skill_json: &str,
        skill_md: &str,
    ) -> Result<String, String> {
        let skills_dir = get_skills_dir();
        let skill_path = skills_dir.join(name);

        if !skill_path.is_dir() {
            return Err(format!("Skill directory '{}' not found", name));
        }

        // Save skill.json
        let json_path = skill_path.join("skill.json");
        // Validate JSON
        let _: SkillManifest = serde_json::from_str(skill_json)
            .map_err(|e| format!("Invalid skill_json format: {}", e))?;
        fs::write(&json_path, skill_json)
            .map_err(|e| format!("Failed to write skill.json: {}", e))?;

        // Save SKILL.md
        if !skill_md.is_empty() {
            let md_path = skill_path.join("SKILL.md");
            fs::write(&md_path, skill_md)
                .map_err(|e| format!("Failed to write SKILL.md: {}", e))?;
        }

        // Refresh catalog entry
        let mut catalog = self.catalog.write().map_err(|e| e.to_string())?;
        catalog.refresh_entry(name, &skills_dir);

        tracing::info!("[SkillRegistry] normalize_skill(save) '{}' completed", name);
        Ok(format!(
            "Skill '{}' has been normalized and saved. It can now be loaded with load_skill.",
            name
        ))
    }

    fn handle_read_file(&self, name: &str, file: &str, start_line: usize, end_line: usize) -> Result<String, String> {
        tracing::info!("[SkillRegistry] read_file '{}/{}' lines {}-{}", name, file, start_line, end_line);
        let catalog = self.catalog.read().map_err(|e| e.to_string())?;
        let entry = catalog
            .get(name)
            .ok_or_else(|| format!("Skill '{}' not found", name))?;

        // Resolve file path (support "SKILL.md", plain filenames, and "tools/xxx")
        let file_path = entry.path.join(file);
        if !file_path.exists() {
            return Err(format!("File '{}' not found in skill '{}'", file, name));
        }

        // Security: ensure resolved path is within the skill directory
        let canonical_skill = entry.path.canonicalize().unwrap_or_else(|_| entry.path.clone());
        let canonical_file = file_path.canonicalize().unwrap_or_else(|_| file_path.clone());
        if !canonical_file.starts_with(&canonical_skill) {
            return Err("Access denied: path traversal detected".to_string());
        }

        let content = fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read '{}': {}", file, e))?;

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        // Clamp range (1-indexed input, convert to 0-indexed)
        let start = start_line.saturating_sub(1).min(total);
        let end = end_line.min(total);

        if start >= end {
            return Ok(format!(
                "[{}/{}] No content in range {}-{} (file has {} lines)",
                file, name, start_line, end_line, total
            ));
        }

        let selected: String = lines[start..end].join("\n");
        let has_more = end < total;

        let mut result = format!(
            "[{} : lines {}-{} of {}]\n{}",
            file, start + 1, end, total, selected
        );

        if has_more {
            result.push_str(&format!(
                "\n\n[还有 {} 行未读。继续: read_file(name=\"{}\", file=\"{}\", start_line={}, end_line={})]",
                total - end, name, file, end + 1, (end + DEFAULT_PAGE_LINES).min(total)
            ));
        }

        Ok(result)
    }

    fn handle_write_file(&self, name: &str, file: &str, content: &str) -> Result<String, String> {
        tracing::info!("[SkillRegistry] write_file '{}/{}' ({} chars)", name, file, content.len());
        let catalog = self.catalog.read().map_err(|e| e.to_string())?;
        let entry = catalog
            .get(name)
            .ok_or_else(|| format!("Skill '{}' not found", name))?;

        let file_path = entry.path.join(file);

        // Security: ensure resolved path is within the skill directory
        let canonical_skill = entry.path.canonicalize().unwrap_or_else(|_| entry.path.clone());
        // For new files, canonicalize the parent
        let parent_dir = file_path.parent().unwrap_or(&entry.path);
        let canonical_parent = parent_dir.canonicalize().unwrap_or_else(|_| parent_dir.to_path_buf());
        
        if !canonical_parent.starts_with(&canonical_skill) {
            return Err("Access denied: path traversal detected".to_string());
        }

        // Write the file
        fs::write(&file_path, content)
            .map_err(|e| format!("Failed to write '{}': {}", file, e))?;

        Ok(format!("Successfully wrote {} characters to '{}/{}'", content.len(), name, file))
    }

    fn handle_replace_file(&self, name: &str, file: &str, target: &str, replacement: &str, start_line: Option<usize>, end_line: Option<usize>, allow_multiple: bool) -> Result<String, String> {
        tracing::info!("[SkillRegistry] replace_file '{}/{}'", name, file);
        let catalog = self.catalog.read().map_err(|e| e.to_string())?;
        let entry = catalog
            .get(name)
            .ok_or_else(|| format!("Skill '{}' not found", name))?;

        let file_path = entry.path.join(file);

        // Security check
        let canonical_skill = entry.path.canonicalize().unwrap_or_else(|_| entry.path.clone());
        let canonical_file = file_path.canonicalize().map_err(|_| format!("File not found: {}", file))?;
        
        if !canonical_file.starts_with(&canonical_skill) {
            return Err("Access denied: path traversal detected".to_string());
        }

        let content = fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read '{}': {}", file, e))?;

        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let total = lines.len();

        let start = start_line.unwrap_or(1).saturating_sub(1).min(total);
        let end = end_line.unwrap_or(total).min(total);

        if start >= end && total > 0 {
            return Err(format!("Invalid line range {}-{} (file has {} lines)", start + 1, end, total));
        }

        let prefix = lines[..start].join("");
        let middle = lines[start..end].join("");
        let suffix = lines[end..].join("");

        let matches = middle.matches(target).count();
        if matches == 0 {
            return Err(format!("Target content not found in lines {}-{} (make sure formatting and whitespace match exactly)", start + 1, end));
        }
        if matches > 1 && !allow_multiple {
            return Err(format!("Target content found {} times in lines {}-{}, but allow_multiple is false", matches, start + 1, end));
        }

        let new_middle = if allow_multiple {
            middle.replace(target, replacement)
        } else {
            middle.replacen(target, replacement, 1)
        };

        let new_content = format!("{}{}{}", prefix, new_middle, suffix);

        fs::write(&file_path, new_content)
            .map_err(|e| format!("Failed to write '{}': {}", file, e))?;

        Ok(format!("Successfully replaced {} occurrence(s) in lines {}-{} of '{}/{}'", matches, start + 1, end, name, file))
    }

    async fn handle_recommend(&self, task: &str) -> Result<String, String> {
        let client = self.client.as_ref().ok_or("No LLM client available for recommendation")?;
        
        let prompt = {
            let catalog = self.catalog.read().map_err(|e| e.to_string())?;
            if catalog.entries.is_empty() {
                return Ok("No external skills available to recommend.".to_string());
            }

            let mut skills_list = String::new();
            for entry in &catalog.entries {
                skills_list.push_str(&format!("Skill Name: {}\nDescription: {}\n\n", entry.name, entry.description));
            }

            format!(
                "You are an intelligent skill router. Your job is to match the user's task to the most relevant external skill(s) from the list below.\n\
                If none match, say so clearly.\n\
                Task: {}\n\n\
                Available Skills:\n{}\n\
                Return the names of the relevant skills and a brief explanation why they match.",
                task, skills_list
            )
        };

        let messages = vec![crate::ai::client::Message {
            role: "user".to_string(),
            content: Some(crate::ai::client::Content::Simple(prompt)),
            ..Default::default()
        }];

        tracing::info!("[SkillRegistry] recommend_skill for task: {:.50}", task);
        let response = client.chat(messages, None).await.map_err(|e| format!("LLM semantic search error: {}", e))?;
        
        Ok(response.content_as_str().to_string())
    }
}

#[async_trait]
impl Skill for SkillRegistrySkill {
    fn name(&self) -> &str {
        "skill_registry"
    }

    fn description(&self) -> &str {
        "Discover, load, and manage external skills. All external skills are stored locally in the 'data/skills/' directory. \
         Use this when your built-in tools cannot fulfill a request and you want to check if an external skill can help. \
         Actions: 'discover_skills' (search available skills by keyword), \
         'recommend_skill' (semantic search using an AI sub-agent, use this if keyword search misses), \
         'load_skill' (activate a skill and register its tools), \
         'normalize_skill' (convert raw files into standard format), \
         'read_file' (paginated reading of a skill file by line range), \
         'write_file' (write or overwrite a file in the skill directory), \
         'replace_file' (replace a specific string in a file)."
    }

    async fn execute(&self, args: Value) -> Result<String, String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| "Missing 'action' parameter".to_string())?;

        match action {
            "discover_skills" => {
                let query = args["query"].as_str().unwrap_or("*");
                self.handle_discover(query)
            }
            "recommend_skill" => {
                let task = args["task"].as_str().ok_or("Missing 'task' for recommendation")?;
                self.handle_recommend(task).await
            }
            "load_skill" => {
                let name = args["name"].as_str().ok_or("Missing 'name'")?;
                self.handle_load(name)
            }
            "normalize_skill" => {
                let name = args["name"].as_str().ok_or("Missing 'name'")?;
                let sub_action = args["sub_action"].as_str().unwrap_or("read");
                match sub_action {
                    "read" => self.handle_normalize_read(name),
                    "save" => {
                        let skill_json =
                            args["skill_json"].as_str().ok_or("Missing 'skill_json'")?;
                        let skill_md = args["skill_md"].as_str().unwrap_or("");
                        self.handle_normalize_save(name, skill_json, skill_md)
                    }
                    _ => Err(format!("Unknown sub_action: {}", sub_action)),
                }
            }
            "read_file" => {
                let name = args["name"].as_str().ok_or("Missing 'name'")?;
                let file = args["file"].as_str().ok_or("Missing 'file'")?;
                let start = args["start_line"].as_u64().unwrap_or(1) as usize;
                let end = args["end_line"].as_u64().unwrap_or((start + DEFAULT_PAGE_LINES - 1) as u64) as usize;
                self.handle_read_file(name, file, start, end)
            }
            "write_file" => {
                let name = args["name"].as_str().ok_or("Missing 'name'")?;
                let file = args["file"].as_str().ok_or("Missing 'file'")?;
                let content = args["content"].as_str().ok_or("Missing 'content'")?;
                self.handle_write_file(name, file, content)
            }
            "replace_file" => {
                let name = args["name"].as_str().ok_or("Missing 'name'")?;
                let file = args["file"].as_str().ok_or("Missing 'file'")?;
                let target = args["target_content"].as_str().ok_or("Missing 'target_content'")?;
                let replacement = args["replacement_content"].as_str().ok_or("Missing 'replacement_content'")?;
                let start_line = args["start_line"].as_u64().map(|v| v as usize);
                let end_line = args["end_line"].as_u64().map(|v| v as usize);
                let allow_multiple = args["allow_multiple"].as_bool().unwrap_or(false);
                self.handle_replace_file(name, file, target, replacement, start_line, end_line, allow_multiple)
            }
            _ => Err(format!(
                "Unknown action: '{}'. Use discover_skills, recommend_skill, load_skill, normalize_skill, read_file, write_file, or replace_file.",
                action
            )),
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
                            "enum": ["discover_skills", "recommend_skill", "load_skill", "normalize_skill", "read_file", "write_file", "replace_file"],
                            "description": "Action to perform"
                        },
                        "query": {
                            "type": "string",
                            "description": "Search query for discover_skills (use '*' for all)"
                        },
                        "task": {
                            "type": "string",
                            "description": "The task requirement for recommend_skill to perform semantic matching"
                        },
                        "name": {
                            "type": "string",
                            "description": "Skill name for load_skill / normalize_skill / read_file / write_file / replace_file"
                        },
                        "file": {
                            "type": "string",
                            "description": "For read_file / write_file / replace_file: filename relative to skill dir"
                        },
                        "content": {
                            "type": "string",
                            "description": "For write_file: The complete content to write to the file"
                        },
                        "start_line": {
                            "type": "integer",
                            "description": "For read_file / replace_file: starting line number (1-indexed). Optional for replace_file to constrain search."
                        },
                        "end_line": {
                            "type": "integer",
                            "description": "For read_file / replace_file: ending line number (inclusive). Optional for replace_file to constrain search."
                        },
                        "target_content": {
                            "type": "string",
                            "description": "For replace_file: the exact string to be replaced (must match whitespace exactly)"
                        },
                        "replacement_content": {
                            "type": "string",
                            "description": "For replace_file: the new string to insert"
                        },
                        "allow_multiple": {
                            "type": "boolean",
                            "description": "For replace_file: if true, replace all occurrences of target_content. Default false."
                        },
                        "sub_action": {
                            "type": "string",
                            "enum": ["read", "save"],
                            "description": "For normalize_skill: 'read' to get raw content, 'save' to persist standardized files"
                        },
                        "skill_json": {
                            "type": "string",
                            "description": "For normalize_skill(save): The generated skill.json content as a JSON string"
                        },
                        "skill_md": {
                            "type": "string",
                            "description": "For normalize_skill(save): The generated SKILL.md content"
                        }
                    },
                    "required": ["action"]
                }
            }
        })
    }
}
