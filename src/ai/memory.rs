use crate::ai::client::Message;
use crate::types::AiConfig;
use rusqlite::{params, Connection, Result};
use std::sync::Mutex;

pub struct MemoryManager {
    conn: Mutex<Connection>,
}

impl MemoryManager {
    pub fn new() -> Self {
        let mut db_path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        db_path.push("data");
        std::fs::create_dir_all(&db_path).ok();
        db_path.push("ameath_memory.db");

        let conn = Connection::open(&db_path).expect("Failed to open database");

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
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_call_id TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )
        .expect("Failed to create tool_traces table");

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

        Self {
            conn: Mutex::new(conn),
        }
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

        Ok((
            l1_count >= config.l1_summary_threshold as i64,
            l2_count >= config.l2_merge_threshold as i64,
        ))
    }

    pub fn add_message(&self, msg: &Message) -> Result<()> {
        self.add_conversation_item(&msg.role, &msg.content, 1)
    }

    pub fn add_conversation_item(&self, role: &str, content: &str, layer: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO conversations (role, content, layer) VALUES (?1, ?2, ?3)",
            params![role, content, layer],
        )?;
        Ok(())
    }

    pub fn add_trace(&self, msg: &Message) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let content = if let Some(tc) = &msg.tool_calls {
            serde_json::to_string(tc).unwrap_or_default()
        } else {
            msg.content.clone()
        };

        conn.execute(
            "INSERT INTO tool_traces (role, content, tool_call_id) VALUES (?1, ?2, ?3)",
            params![msg.role, content, msg.tool_call_id],
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
        conn.execute("DELETE FROM tool_traces", [])?;
        Ok(())
    }

    pub fn get_context(&self, limit: usize) -> Result<Vec<Message>> {
        let mut context = Vec::new();

        // 1. Get Facts
        let facts = self.get_facts()?;
        if !facts.is_empty() {
            let facts_str = facts
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join("\n");
            context.push(Message {
                role: "system".to_string(),
                content: format!("Known Facts about User:\n{}", facts_str),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // 2. Get L3 (Latest High-level Summary)
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT content FROM summaries WHERE layer >= 3 ORDER BY id DESC LIMIT 1")?;
        let l3_opt: Option<String> = stmt.query_row([], |r| r.get(0)).ok();
        if let Some(l3) = l3_opt {
            context.push(Message {
                role: "system".to_string(),
                content: format!("Long-term Summary:\n{}", l3),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // 3. Get Recent L2 Summaries (from conversations layer=2)
        let mut stmt = conn.prepare(
            "SELECT content FROM conversations WHERE layer = 2 ORDER BY id DESC LIMIT 3",
        )?;
        let l2_rows = stmt.query_map([], |row| row.get(0))?;
        let mut l2_summaries: Vec<String> = Vec::new();
        for row in l2_rows {
            if let Ok(s) = row {
                l2_summaries.push(s);
            }
        }
        if !l2_summaries.is_empty() {
            l2_summaries.reverse();
            context.push(Message {
                role: "system".to_string(),
                content: format!("Recent Context Summary:\n{}", l2_summaries.join("\n")),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // 4. Get Active Tool Traces (Volatile)
        let mut stmt =
            conn.prepare("SELECT role, content, tool_call_id FROM tool_traces ORDER BY id ASC")?; // Traces should be chronological? Or reverse and then reverse back?
                                                                                                  // Actually, if we clear them, we just want all of them.
        let trace_rows = stmt.query_map([], |row| {
            Ok(Message {
                role: row.get(0)?,
                content: row.get(1)?,
                tool_calls: None, // Simplified for now
                tool_call_id: row.get(2)?,
            })
        })?;
        let mut traces = Vec::new();
        for row in trace_rows {
            traces.push(row?);
        }
        if !traces.is_empty() {
            context.push(Message {
                role: "system".to_string(),
                content: "--- START OF CURRENT TOOL EXECUTION LOG ---".to_string(),
                tool_calls: None,
                tool_call_id: None,
            });
            context.extend(traces);
            context.push(Message {
                role: "system".to_string(),
                content: "--- END OF TOOL EXECUTION LOG ---".to_string(),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // 5. Get Recent L1 Core Dialogue (Limit to recent N)
        let mut stmt = conn.prepare(
            "SELECT role, content FROM conversations WHERE layer = 1 ORDER BY id DESC LIMIT ?",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(Message {
                role: row.get(0)?,
                content: row.get(1)?,
                tool_calls: None,
                tool_call_id: None,
            })
        })?;

        let mut history: Vec<Message> = Vec::new();
        for row in rows {
            history.push(row?);
        }
        history.reverse();
        context.extend(history);

        Ok(context)
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
        conn.execute(&query, params![layer, upto_id])?;
        Ok(())
    }

    pub fn vacuum(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("VACUUM", [])?;
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
        conn.execute(
            "DELETE FROM conversations 
             WHERE layer = 1 
             AND summarized = 1 
             AND id NOT IN (SELECT id FROM conversations WHERE layer = 1 ORDER BY id DESC LIMIT ?1)",
            params![keep_count],
        )?;

        // Prune Layer 2 (Summaries) -> Delete if compacted AND not in the last N
        conn.execute(
            "DELETE FROM conversations 
             WHERE layer = 2 
             AND compacted = 1 
             AND id NOT IN (SELECT id FROM conversations WHERE layer = 2 ORDER BY id DESC LIMIT ?1)",
            params![keep_count],
        )?;

        Ok(())
    }
}
