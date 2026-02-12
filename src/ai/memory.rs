use crate::ai::client::Message;
use rusqlite::{params, Connection, Result};
use std::path::PathBuf;

pub struct MemoryManager {
    db_path: PathBuf,
}

impl MemoryManager {
    pub fn new() -> Self {
        let mut db_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        db_path.push("data");
        std::fs::create_dir_all(&db_path).ok();
        db_path.push("memory.db");

        let manager = Self { db_path };
        manager.init_db().expect("Failed to initialize database");
        manager
    }

    fn init_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Layer 1: Conversations (Core Dialogue)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Layer 2a: Tool Traces (Execution Logs)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tool_traces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_call_id TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Layer 2b/3: Summaries (Cognitive Abstraction)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                layer INTEGER NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Fact Store: Persistent Knowledge Board
        conn.execute(
            "CREATE TABLE IF NOT EXISTS facts (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        Ok(())
    }

    pub fn add_message(&self, msg: &Message) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO conversations (role, content) VALUES (?1, ?2)",
            params![msg.role, msg.content],
        )?;
        Ok(())
    }

    pub fn get_recent_history(&self, limit: usize) -> Result<Vec<Message>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt =
            conn.prepare("SELECT role, content FROM conversations ORDER BY id DESC LIMIT ?")?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(Message {
                role: row.get(0)?,
                content: row.get(1)?,
            })
        })?;

        let mut history: Vec<Message> = Vec::new();
        for row in rows {
            history.push(row?);
        }
        history.reverse();
        Ok(history)
    }

    pub fn set_fact(&self, key: &str, value: &str) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT OR REPLACE INTO facts (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)",
            params![key, value],
        )?;
        Ok(())
    }
}
