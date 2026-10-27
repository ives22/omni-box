use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtoolsDoc {
    pub _id: String,
    pub _rev: Option<String>,
    pub data: serde_json::Value,
}

pub struct AppDatabase {
    pub conn: Connection,
}

impl AppDatabase {
    pub fn init(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(db_path).map_err(|e| e.to_string())?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS kv_docs (
                plugin_id TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                rev TEXT NOT NULL,
                data_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (plugin_id, doc_id)
            );

            CREATE TABLE IF NOT EXISTS usage_stats (
                item_id TEXT PRIMARY KEY,
                hit_count INTEGER DEFAULT 1,
                last_used_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS vault_config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                argon2_salt BLOB NOT NULL,
                verify_nonce BLOB NOT NULL,
                verify_ciphertext BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS vault_items (
                id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                title TEXT NOT NULL,
                subtitle TEXT NOT NULL,
                nonce BLOB NOT NULL,
                ciphertext BLOB NOT NULL,
                updated_at INTEGER NOT NULL
            );
            ",
        )
        .map_err(|e| e.to_string())?;

        Ok(Self { conn })
    }

    pub fn record_usage(&self, item_id: &str) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT INTO usage_stats (item_id, hit_count, last_used_at)
                 VALUES (?1, 1, ?2)
                 ON CONFLICT(item_id) DO UPDATE SET
                    hit_count = hit_count + 1,
                    last_used_at = excluded.last_used_at",
                params![item_id, now],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_frecency_map(&self) -> HashMap<String, i64> {
        let mut map = HashMap::new();
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT item_id, hit_count FROM usage_stats")
        {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            }) {
                for r in rows.flatten() {
                    map.insert(r.0, r.1);
                }
            }
        }
        map
    }

    pub fn kv_put(&self, plugin_id: &str, doc_id: &str, data_json: &str) -> Result<String, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let rev = format!("1-{:x}", now);

        self.conn
            .execute(
                "INSERT INTO kv_docs (plugin_id, doc_id, rev, data_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(plugin_id, doc_id) DO UPDATE SET
                    rev = excluded.rev,
                    data_json = excluded.data_json,
                    updated_at = excluded.updated_at",
                params![plugin_id, doc_id, rev, data_json, now],
            )
            .map_err(|e| e.to_string())?;

        Ok(rev)
    }

    pub fn kv_get(&self, plugin_id: &str, doc_id: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT data_json FROM kv_docs WHERE plugin_id = ?1 AND doc_id = ?2",
                params![plugin_id, doc_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
}
