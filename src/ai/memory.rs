use crate::ai::client::{Content, Message};
use crate::types::AiConfig;
use rusqlite::{params, Connection, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub struct MemoryManager {
    conn: Mutex<Connection>,
    summarization_in_progress: AtomicBool,
}

pub struct SummarizationGuard<'a> {
    flag: &'a AtomicBool,
}

impl Drop for SummarizationGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}

impl MemoryManager {
    pub fn new() -> Self {
        let mut db_path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        db_path.push("data");
        std::fs::create_dir_all(&db_path).ok();
        db_path.push("ameath_memory.db");

        let conn = Connection::open(&db_path).expect("Failed to open database");

        tracing::info!("[Memory] Database initialized at {:?}", db_path);

        Self::from_connection(conn)
    }

    #[cfg(test)]
    pub fn new_in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("Failed to open in-memory database");
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Self {
        Self::initialize_schema(&conn);

        Self {
            conn: Mutex::new(conn),
            summarization_in_progress: AtomicBool::new(false),
        }
    }

    fn initialize_schema(conn: &Connection) {

        // Layer 1 & 2: Conversations (Core Dialogue & L1 Summaries)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                layer INTEGER DEFAULT 1, -- 1=Dialogue, 2=L1 Summary
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                summarized INTEGER DEFAULT 0, -- For Layer 1 -> 2
                compacted INTEGER DEFAULT 0   -- For Layer 2 -> 3
            )",
            [],
        )
        .expect("Failed to create conversations table");

        // Tool Traces: Volatile Execution Logs
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tool_traces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id TEXT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_call_id TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )
        .expect("Failed to create tool_traces table");
        
        // Migration: ensure tool_calls column exists
        let _ = conn.execute("ALTER TABLE tool_traces ADD COLUMN tool_calls TEXT", []);
        let _ = conn.execute("ALTER TABLE tool_traces ADD COLUMN request_id TEXT", []);
        
        // Migration: ensure multi-modal columns exist
        let _ = conn.execute("ALTER TABLE conversations ADD COLUMN images_json TEXT", []);
        let _ = conn.execute("ALTER TABLE conversations ADD COLUMN images_desc TEXT", []);

        // Layer 3: Long-term Summaries (Condensed Knowledge)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                layer INTEGER NOT NULL, -- Keep for compatibility, mostly 3
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )
        .expect("Failed to create summaries table");

        // Fact Store: Persistent Knowledge Board
        conn.execute(
            "CREATE TABLE IF NOT EXISTS facts (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )
        .expect("Failed to create facts table");

        // Entity Graph: Relational Memory
        conn.execute(
            "CREATE TABLE IF NOT EXISTS entity_graph (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source TEXT NOT NULL,
                relation TEXT NOT NULL,
                target TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(source, relation, target)
            )",
            [],
        )
        .expect("Failed to create entity_graph table");

        // Create index for fast Graph queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_entity_graph_source ON entity_graph(source)",
            [],
        )
        .expect("Failed to create index on entity_graph(source)");

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_entity_graph_target ON entity_graph(target)",
            [],
        )
        .expect("Failed to create index on entity_graph(target)");
    }

    pub fn check_thresholds(&self, config: &AiConfig) -> Result<(bool, bool)> {
        let conn = self.conn.lock().unwrap();

        // Check L1 (Un-summarized messages in conversations layer=1)
        let l1_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM conversations WHERE layer = 1 AND summarized = 0",
            [],
            |r| r.get(0),
        )?;

        // Check L2 (Un-compacted summaries in conversations layer=2)
        let l2_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM conversations WHERE layer = 2 AND compacted = 0",
            [],
            |r| r.get(0),
        )?;

        let l1_hit = l1_count >= config.l1_summary_threshold as i64;
        let l2_hit = l2_count >= config.l2_merge_threshold as i64;
        if l1_hit || l2_hit {
            tracing::info!("[Memory] Threshold check: L1={}/{} (hit={}), L2={}/{} (hit={})",
                l1_count, config.l1_summary_threshold, l1_hit, l2_count, config.l2_merge_threshold, l2_hit);
        }

        Ok((l1_hit, l2_hit))
    }

    pub fn add_message(&self, msg: &Message) -> Result<()> {
        self.add_message_returning_id(msg)?;
        Ok(())
    }

    pub fn add_message_returning_id(&self, msg: &Message) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        
        let content_str = msg.content_as_str();
        
        let (images_json, images_desc): (Option<String>, Option<String>) = match &msg.content {
            Some(Content::Multimodal(parts)) => {
                let images: Vec<_> = parts.iter().filter_map(|p| {
                    if let crate::ai::client::ContentPart::ImageUrl { image_url } = p {
                        Some(image_url.clone())
                    } else {
                        None
                    }
                }).collect();
                if images.is_empty() {
                    (None, None)
                } else {
                    (Some(serde_json::to_string(&images).unwrap_or_default()), None)
                }
            }
            _ => (None, None),
        };

        // Note: For multi-modal messages, images_desc defaults to NULL initially
        conn.execute(
            "INSERT INTO conversations (role, content, layer, images_json, images_desc) VALUES (?1, ?2, 1, ?3, ?4)",
            params![msg.role, content_str, images_json, images_desc],
        )?;
        let id = conn.last_insert_rowid();
        tracing::debug!("[Memory] add_message: role={}, chars={}, images={}, id={}",
            msg.role, content_str.len(), images_json.is_some(), id);
        Ok(id)
    }

    pub fn add_conversation_item(&self, role: &str, content: &str, layer: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO conversations (role, content, layer) VALUES (?1, ?2, ?3)",
            params![role, content, layer],
        )?;
        Ok(())
    }

    pub fn update_image_desc(&self, msg_id: i64, desc: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE conversations SET images_desc = ?1 WHERE id = ?2",
            params![desc, msg_id],
        )?;
        Ok(())
    }

    pub fn prune_expired_images(&self, keep_limit: usize) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE conversations 
             SET images_json = NULL 
             WHERE id NOT IN (
                 SELECT id FROM conversations 
                 WHERE images_json IS NOT NULL 
                 ORDER BY id DESC 
                 LIMIT ?1
             ) AND images_json IS NOT NULL",
            params![keep_limit],
        )?;
        Ok(())
    }

    pub fn add_trace(&self, msg: &Message) -> Result<()> {
        self.add_trace_for_request("", msg)
    }

    pub fn add_trace_for_request(&self, request_id: &str, msg: &Message) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let content = if let Some(_tc) = &msg.tool_calls {
            // Keep original content if present, or use empty
            msg.content_as_str().to_string()
        } else {
            msg.content_as_str().to_string()
        };
        
        let tool_calls_json = msg.tool_calls.as_ref().map(|tc| serde_json::to_string(tc).unwrap_or_default());

        conn.execute(
            "INSERT INTO tool_traces (request_id, role, content, tool_call_id, tool_calls) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![request_id, msg.role, content, msg.tool_call_id, tool_calls_json],
        )?;
        Ok(())
    }

    pub fn add_summary(&self, content: &str, layer: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO summaries (content, layer) VALUES (?1, ?2)",
            params![content, layer],
        )?;
        Ok(())
    }

    pub fn clear_traces(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute("DELETE FROM tool_traces", [])?;
        if deleted > 0 {
            tracing::debug!("[Memory] Cleared {} tool traces", deleted);
        }
        Ok(())
    }

    pub fn clear_traces_for_request(&self, request_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM tool_traces WHERE request_id = ?1",
            params![request_id],
        )?;
        if deleted > 0 {
            tracing::debug!(
                "[Memory] Cleared {} tool traces for request {}",
                deleted,
                request_id
            );
        }
        Ok(())
    }

    pub fn get_context(&self, limit: usize, allow_images: bool) -> Result<Vec<Message>> {
        self.get_context_for_request(limit, allow_images, None)
    }

    pub fn get_context_for_request(
        &self,
        limit: usize,
        allow_images: bool,
        request_id: Option<&str>,
    ) -> Result<Vec<Message>> {
        fn trace_message_from_row(row: &rusqlite::Row<'_>) -> Result<Message> {
            let role: String = row.get(0)?;
            let content_str: String = row.get(1)?;
            let tool_call_id: Option<String> = row.get(2)?;
            let tool_calls_json: Option<String> = row.get(3)?;

            let tool_calls = tool_calls_json.and_then(|s| serde_json::from_str(&s).ok());

            Ok(Message {
                role,
                content: if content_str.is_empty() && tool_calls.is_some() {
                    None
                } else {
                    Some(Content::Simple(content_str))
                },
                tool_calls,
                tool_call_id,
                ..Default::default()
            })
        }

        let mut context = Vec::new();

        // 1. Get Facts
        let facts = self.get_facts()?;
        let facts_instruction = "\n[Instruction]\n\
            You have access to a permanent Fact Board and Entity Graph. \
            As a proactive AI, you MUST independently use the 'update_fact_board' tool (action='set' or 'add_relation') to remember new persistent facts, preferences, or relationships you learn about the user during conversations. \
            If existing facts contradict new information, 'update' or 'delete' them. \
            Do not wait for the user to explicitly tell you to 'remember' something if it is obviously a long-term preference or important context.";

        if !facts.is_empty() {
            let facts_str = facts
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join("\n");
            context.push(Message {
                role: "system".to_string(),
                content: Some(Content::Simple(format!("[Current Fact Board / Known Facts]\n{}{}", facts_str, facts_instruction))),
                ..Default::default()
            });
        } else {
            context.push(Message {
                role: "system".to_string(),
                content: Some(Content::Simple(format!("[Current Fact Board / Known Facts]\n(Currently Empty){}", facts_instruction))),
                ..Default::default()
            });
        }

        // 2. Get L3 (Latest High-level Summary)
        let conn = self.conn.lock().unwrap();

        let l3_opt: Option<String> = {
            let mut stmt = conn.prepare(
                "SELECT content FROM summaries WHERE layer >= 3 ORDER BY id DESC LIMIT 1",
            )?;
            stmt.query_row([], |r| r.get(0)).ok()
        };
        let has_l3 = l3_opt.is_some();
        if let Some(l3) = l3_opt {
            context.push(Message {
                role: "system".to_string(),
                content: Some(Content::Simple(format!("Long-term Summary:\n{}", l3))),
                ..Default::default()
            });
        }

        // 3. Get Recent L2 Summaries (from conversations layer=2)
        let mut l2_summaries: Vec<String> = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT content FROM conversations WHERE layer = 2 ORDER BY id DESC LIMIT 3",
            )?;
            let l2_rows = stmt.query_map([], |row| row.get(0))?;
            for row in l2_rows {
                if let Ok(s) = row {
                    l2_summaries.push(s);
                }
            }
        }
        if !l2_summaries.is_empty() {
            l2_summaries.reverse();
                context.push(Message {
                    role: "system".to_string(),
                    content: Some(Content::Simple(format!(
                        "Recent Context Summary:\n{}",
                        l2_summaries.join("\n")
                    ))),
                    ..Default::default()
                });
        }

        // 4. Get Active Tool Traces (Volatile)
        let mut traces = Vec::new();
        {
            let trace_query = if request_id.is_some() {
                "SELECT role, content, tool_call_id, tool_calls FROM tool_traces WHERE request_id = ?1 ORDER BY id ASC"
            } else {
                "SELECT role, content, tool_call_id, tool_calls FROM tool_traces ORDER BY id ASC"
            };
            let mut stmt = conn.prepare(trace_query)?;
            if let Some(request_id) = request_id {
                let trace_rows = stmt.query_map(params![request_id], trace_message_from_row)?;
                for row in trace_rows {
                    traces.push(row?);
                }
            } else {
                let trace_rows = stmt.query_map([], trace_message_from_row)?;
                for row in trace_rows {
                    traces.push(row?);
                }
            }
        }
        if !traces.is_empty() {
            context.push(Message {
                role: "system".to_string(),
                content: Some(Content::Simple("--- START OF CURRENT TOOL EXECUTION LOG ---".to_string())),
                ..Default::default()
            });
            context.extend(traces);
            context.push(Message {
                role: "system".to_string(),
                content: Some(Content::Simple("--- END OF TOOL EXECUTION LOG ---".to_string())),
                ..Default::default()
            });
        }

        // 5. Get Recent L1 Core Dialogue (Limit to recent N)
        let mut history: Vec<Message> = Vec::new();
        let mut recent_user_text = String::new();
        {
            let mut stmt = conn.prepare(
                "SELECT role, content, images_json, images_desc FROM conversations WHERE layer = 1 ORDER BY id DESC LIMIT ?",
            )?;
            let rows = stmt.query_map(params![limit], |row| {
                let role: String = row.get(0)?;
                let text_content: String = row.get(1)?;
                let images_json: Option<String> = row.get(2)?;
                let images_desc: Option<String> = row.get(3)?;
                
                let mut content = Content::Simple(text_content.clone());
                
                if let Some(json_str) = images_json {
                    if allow_images {
                        if let Ok(images) = serde_json::from_str::<Vec<crate::ai::client::ImageUrl>>(&json_str) {
                            if !images.is_empty() {
                                let mut parts = vec![crate::ai::client::ContentPart::Text { text: text_content.clone() }];
                                for img in images {
                                    parts.push(crate::ai::client::ContentPart::ImageUrl { image_url: img });
                                }
                                content = Content::Multimodal(parts);
                            }
                        }
                    } else {
                        // Fallback to text description since allow_images is false
                        let desc = images_desc.unwrap_or_else(|| "[图片内容处理中/暂不可见]".to_string());
                        content = Content::Simple(format!("{}\n[历史多模态附图摘要: {}]", text_content, desc));
                    }
                } else if let Some(desc) = images_desc {
                     // images_json is expired (or missing from multi-modal inputs originally somehow) but we have description
                     content = Content::Simple(format!("{}\n[历史多模态附图摘要: {}]", text_content, desc));
                }

                Ok(Message {
                    role,
                    content: Some(content),
                    ..Default::default()
                })
            })?;

            for row in rows {
                let msg: Message = row?;
                if msg.role == "user" {
                    let text = msg.content_as_str();
                    recent_user_text.push_str(text);
                    recent_user_text.push(' ');
                    history.push(msg);
                } else if msg.role == "assistant" {
                    let content = msg.content_as_str().to_string();
                    let delimiter = "\n\n--- FC 调用过程记录 ---\n";
                    if let Some(idx) = content.find(delimiter) {
                        let original_response = &content[..idx];
                        let trace_content = &content[idx + delimiter.len()..];
                        
                        // Push the clean assistant message
                        history.push(Message {
                            role: "assistant".to_string(),
                            content: Some(Content::Simple(original_response.to_string())),
                            ..Default::default()
                        });

                        // Create the system context message for the parsed traces
                        let mut system_trace_text = format!("[历史工具调用背景：该轮对话中模型调用的工具及其结果如下]\n{}", trace_content);
                        
                        // Special handling for reminders
                        if trace_content.contains("Tool: schedule_reminder") {
                            system_trace_text.push_str("\n*注：此条目仅记录定时任务已成功提交至调度器，不代表定时任务此时已触发生效，请勿在回复中模仿工具调用格式。*");
                        } else {
                            system_trace_text.push_str("\n*注：上述信息为历史系统执行记录，作为当前上下文参考，请勿在回复中模仿工具调用格式。*");
                        }

                        // Push the system trace message *before* the assistant message in the vector
                        // Since `history` gets reversed later, we want the system trace to appear 
                        // chronologically *after* the assistant message if we push them here, 
                        // it means pushing assistant then system. After reverse, system will be before assistant.
                        // Wait, chronological order in DB is oldest to newest. Oh, the limit query order is DESC.
                        // So the first row is the *newest*.
                        // When reading DESC, we read: Msg N, Msg N-1, Msg N-2...
                        // After `history.reverse()`, it becomes: Msg N-2, Msg N-1, Msg N
                        // We want the AI to see: Assistant Reply -> System Trace
                        // So in the `history` vector (which is DESC), we should push: System Trace -> Assistant Reply.
                        // Let's verify: 
                        // history = [System Trace, Assistant Reply] -> reverse -> [Assistant Reply, System Trace]. Correct.
                        
                        history.pop(); // Remove the assistant message we just pushed
                        
                        history.push(Message {
                            role: "system".to_string(),
                            content: Some(Content::Simple(system_trace_text)),
                            ..Default::default()
                        });

                        history.push(Message {
                            role: "assistant".to_string(),
                            content: Some(Content::Simple(original_response.to_string())),
                            ..Default::default()
                        });

                    } else {
                        history.push(msg);
                    }
                } else {
                    history.push(msg);
                }
            }
        } // `stmt` for L1 dialogue is dropped here
        history.reverse();

        // --- 5.5 Entity Graph Retrieval (Lightweight Mem0-like) ---
        // We drop locks early to call our own connection again if needed.
        // Because we bound statements inside blocks {}, they are already dropped here!
        drop(conn);

        // 2. Naive Entity Extraction: Since we don't have an NLP library,
        // we'll fetch ALL existing entity names from the DB and check if they occur in `recent_user_text`.
        // This is extremely fast for <10k entities.
        let mut entities_in_prompt = Vec::new();
        if !recent_user_text.is_empty() {
            let conn2 = self.conn.lock().unwrap();
            // Get unique entities
            let mut stmt2 = conn2.prepare(
                 "SELECT DISTINCT source FROM entity_graph UNION SELECT DISTINCT target FROM entity_graph"
             )?;
            if let Ok(mut g_rows) = stmt2.query([]) {
                while let Ok(Some(g_row)) = g_rows.next() {
                    if let Ok(entity) = g_row.get::<_, String>(0) {
                        // Simple substring match
                        if recent_user_text.contains(&entity) {
                            entities_in_prompt.push(entity);
                        }
                    }
                }
            }
            drop(stmt2);
            drop(conn2);
        }

        // 3. Fetch 1-hop relations and inject
        if !entities_in_prompt.is_empty() {
            let entity_strs: Vec<&str> = entities_in_prompt.iter().map(|s| s.as_str()).collect();
            if let Ok(relations) = self.get_relations_for_entities(&entity_strs) {
                if !relations.is_empty() {
                    let mut graph_ctx = String::new();
                    for (src, rel, tgt) in relations {
                        graph_ctx.push_str(&format!("- [{}] -> [{}] -> [{}]\n", src, rel, tgt));
                    }
                    context.push(Message {
                        role: "system".to_string(),
                        content: if graph_ctx.is_empty() {
                            None
                        } else {
                            Some(Content::Simple(format!(
                                "Relevant Graph Connections:\n{}",
                                graph_ctx
                            )))
                        },
                        ..Default::default()
                    });
                }
            }
        }

        context.extend(history);

        tracing::debug!("[Memory] get_context assembled: {} messages total (facts={}, L3={}, L2={}, traces, L1 limit={})",
            context.len(), if facts.is_empty() { 0 } else { 1 }, if has_l3 { 1 } else { 0 },
            l2_summaries.len(), limit);

        Ok(context)
    }

    pub fn get_l1_unsummarized_batch(&self, limit: usize) -> Result<Vec<(i64, Message)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, role, content, images_desc FROM conversations WHERE layer = 1 AND summarized = 0 ORDER BY id ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            let id: i64 = row.get(0)?;
            let role: String = row.get(1)?;
            let text_content: String = row.get(2)?;
            let images_desc: Option<String> = row.get(3)?;

            let content = if let Some(desc) = images_desc {
                Content::Simple(format!(
                    "{}\n[历史多模态附图摘要: {}]",
                    text_content, desc
                ))
            } else {
                Content::Simple(text_content)
            };

            Ok((
                id,
                Message {
                    role,
                    content: Some(content),
                    ..Default::default()
                },
            ))
        })?;

        let mut batch = Vec::new();
        for row in rows {
            batch.push(row?);
        }
        Ok(batch)
    }

    pub fn mark_l1_summarized(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let conn = self.conn.lock().unwrap();
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "UPDATE conversations SET summarized = 1 WHERE layer = 1 AND id IN ({})",
            placeholders
        );
        conn.execute(&query, rusqlite::params_from_iter(ids.iter()))?;
        Ok(())
    }

    pub fn try_start_summarization(&self) -> Option<SummarizationGuard<'_>> {
        self.summarization_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| SummarizationGuard {
                flag: &self.summarization_in_progress,
            })
    }

    pub fn get_latest_l3(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT content FROM summaries WHERE layer >= 3 ORDER BY id DESC LIMIT 1")?;
        let l3_opt: Option<String> = stmt.query_row([], |r| r.get(0)).ok();
        Ok(l3_opt)
    }

    pub fn set_fact(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO facts (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_fact(&self, key: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM facts WHERE key = ?1", params![key])?;
        Ok(())
    }

    pub fn get_facts(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM facts")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut facts = Vec::new();
        for row in rows {
            facts.push(row?);
        }
        Ok(facts)
    }

    // --- Entity Graph Methods ---

    pub fn add_relation(&self, source: &str, relation: &str, target: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO entity_graph (source, relation, target, updated_at) 
             VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)",
            params![source, relation, target],
        )?;
        Ok(())
    }

    pub fn delete_relation(&self, source: &str, relation: &str, target: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM entity_graph WHERE source = ?1 AND relation = ?2 AND target = ?3",
            params![source, relation, target],
        )?;
        Ok(())
    }

    /// Retrieve 1-hop relations for a list of entity names.
    pub fn get_relations_for_entities(
        &self,
        entities: &[&str],
    ) -> Result<Vec<(String, String, String)>> {
        if entities.is_empty() {
            return Ok(Vec::new());
        }

        // This limit is to avoid context overflow, prioritizing recently updated relations
        let limit = 20;

        let placeholders = entities.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT source, relation, target 
             FROM entity_graph 
             WHERE source IN ({p}) OR target IN ({p}) 
             ORDER BY updated_at DESC LIMIT {l}",
            p = placeholders,
            l = limit
        );

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&query)?;

        let params = rusqlite::params_from_iter(entities.iter().chain(entities.iter()));
        let rows = stmt.query_map(params, |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;

        let mut relations = Vec::new();
        for row in rows {
            relations.push(row?);
        }
        Ok(relations)
    }

    pub fn get_l2_uncompacted(&self, limit: usize) -> Result<Vec<(i64, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, content FROM conversations WHERE layer = 2 AND compacted = 0 ORDER BY id ASC LIMIT ?",
        )?;
        let rows = stmt.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn mark_l2_compacted(&self, ids: &[i64]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE conversations SET compacted = 1 WHERE id = ?1",
                params![id],
            )?;
        }
        tx.commit()?;
        tracing::info!("[Memory] Marked {} L2 items as compacted", ids.len());
        Ok(())
    }

    pub fn get_latest_id_for_layer(&self, layer: i32) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT MAX(id) FROM conversations WHERE layer = ?1")?;
        let id_opt: Option<i64> = stmt.query_row(params![layer], |r| r.get(0)).ok();
        Ok(id_opt)
    }

    pub fn mark_layer_processed(&self, layer: i32, upto_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let field = match layer {
            1 => "summarized",
            2 => "compacted",
            _ => return Ok(()),
        };
        let query = format!(
            "UPDATE conversations SET {} = 1 WHERE layer = ?1 AND id <= ?2",
            field
        );
        let affected = conn.execute(&query, params![layer, upto_id])?;
        tracing::info!("[Memory] mark_layer_processed: layer={}, upto_id={}, affected={}", layer, upto_id, affected);
        Ok(())
    }

    pub fn vacuum(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("VACUUM", [])?;
        tracing::debug!("[Memory] Database vacuumed");
        Ok(())
    }

    pub fn get_recent_history(&self, limit: usize) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT role, content FROM (
                SELECT id, role, content FROM conversations 
                WHERE layer = 1 
                ORDER BY id DESC 
                LIMIT ?1
            ) ORDER BY id ASC",
        )?;

        let rows = stmt.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut history = Vec::new();
        for row in rows {
            history.push(row?);
        }

        // Optimize: Group by "Exchange" (User followed by responses) and reverse groups
        let mut grouped = Vec::new();
        let mut current_group = Vec::new();
        for item in history {
            // A new group starts with a "user" message
            if item.0 == "user" && !current_group.is_empty() {
                grouped.push(current_group);
                current_group = Vec::new();
            }
            current_group.push(item);
        }
        if !current_group.is_empty() {
            grouped.push(current_group);
        }

        grouped.reverse(); // Newest groups first

        Ok(grouped.into_iter().flatten().collect())
    }

    pub fn prune_layers(&self, keep_count: usize) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Prune Layer 1 (Dialogue) -> Delete if summarized AND not in the last N
        let l1_deleted = conn.execute(
            "DELETE FROM conversations 
             WHERE layer = 1 
             AND summarized = 1 
             AND id NOT IN (SELECT id FROM conversations WHERE layer = 1 ORDER BY id DESC LIMIT ?1)",
            params![keep_count],
        )?;

        // Prune Layer 2 (Summaries) -> Delete if compacted AND not in the last N
        let l2_deleted = conn.execute(
            "DELETE FROM conversations 
             WHERE layer = 2 
             AND compacted = 1 
             AND id NOT IN (SELECT id FROM conversations WHERE layer = 2 ORDER BY id DESC LIMIT ?1)",
            params![keep_count],
        )?;

        // Prune Layer 3 (Long-term Summaries) -> Keep latest N versions
        let l3_deleted = conn.execute(
            "DELETE FROM summaries 
             WHERE id NOT IN (SELECT id FROM summaries ORDER BY id DESC LIMIT ?1)",
            params![20], // Keep last 20 versions of L3
        )?;

        if l1_deleted > 0 || l2_deleted > 0 || l3_deleted > 0 {
            tracing::info!("[Memory] prune_layers: L1 deleted={}, L2 deleted={}, L3 deleted={} (keep={})",
                l1_deleted, l2_deleted, l3_deleted, keep_count);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::client::{Content, Message, ToolCall, ToolFunction};

    fn text_message(role: &str, text: &str) -> Message {
        Message {
            role: role.to_string(),
            content: Some(Content::Simple(text.to_string())),
            ..Default::default()
        }
    }

    #[test]
    fn request_scoped_traces_are_isolated() {
        let memory = MemoryManager::new_in_memory();
        let req_a = "req-a";
        let req_b = "req-b";

        let assistant_a = Message {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "call-a".to_string(),
                r#type: "function".to_string(),
                function: ToolFunction {
                    name: "tool_a".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
            ..Default::default()
        };
        let assistant_b = Message {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: "call-b".to_string(),
                r#type: "function".to_string(),
                function: ToolFunction {
                    name: "tool_b".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
            ..Default::default()
        };

        memory.add_trace_for_request(req_a, &assistant_a).unwrap();
        memory.add_trace_for_request(req_b, &assistant_b).unwrap();

        let context_a = memory
            .get_context_for_request(10, false, Some(req_a))
            .unwrap();
        let trace_names_a: Vec<String> = context_a
            .iter()
            .filter_map(|msg| msg.tool_calls.as_ref())
            .flat_map(|calls: &Vec<ToolCall>| calls.iter().map(|call| call.function.name.clone()))
            .collect();

        assert!(trace_names_a.contains(&"tool_a".to_string()));
        assert!(!trace_names_a.contains(&"tool_b".to_string()));

        memory.clear_traces_for_request(req_a).unwrap();
        let context_b = memory
            .get_context_for_request(10, false, Some(req_b))
            .unwrap();
        let trace_names_b: Vec<String> = context_b
            .iter()
            .filter_map(|msg| msg.tool_calls.as_ref())
            .flat_map(|calls: &Vec<ToolCall>| calls.iter().map(|call| call.function.name.clone()))
            .collect();

        assert!(trace_names_b.contains(&"tool_b".to_string()));
    }

    #[test]
    fn l1_batch_marking_only_affects_selected_messages() {
        let memory = MemoryManager::new_in_memory();
        memory.add_message(&text_message("user", "oldest")).unwrap();
        memory.add_message(&text_message("assistant", "middle")).unwrap();
        memory.add_message(&text_message("user", "newest")).unwrap();

        let batch = memory.get_l1_unsummarized_batch(2).unwrap();
        let batch_texts: Vec<String> = batch
            .iter()
            .map(|(_, msg): &(i64, Message)| msg.content_as_str().to_string())
            .collect();
        assert_eq!(batch_texts, vec!["oldest".to_string(), "middle".to_string()]);

        let processed_ids: Vec<i64> = batch.iter().map(|(id, _)| *id).collect();
        memory.mark_l1_summarized(&processed_ids).unwrap();

        let remaining = memory.get_l1_unsummarized_batch(10).unwrap();
        let remaining_texts: Vec<String> = remaining
            .iter()
            .map(|(_, msg): &(i64, Message)| msg.content_as_str().to_string())
            .collect();
        assert_eq!(remaining_texts, vec!["newest".to_string()]);
    }

    #[test]
    fn summarization_guard_is_exclusive() {
        let memory = MemoryManager::new_in_memory();

        let guard = memory.try_start_summarization().unwrap();
        assert!(memory.try_start_summarization().is_none());

        drop(guard);
        assert!(memory.try_start_summarization().is_some());
    }
}
