//! SQLite store: one file, WAL. Every table is (id, json, ord): the record is
//! a JSON document and `ord` is what lists sort by (created_at). FTS5 indexes
//! transcript text so the library search reads what was said.
//!
//! ponytail: document tables instead of one column per field. The whole
//! library is a few thousand rows and the UI works from a snapshot; per-column
//! SQL buys nothing here. Split a table out when a query needs an index.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

use crate::model::Settings;

pub const TABLES: &[&str] = &["sources", "channels", "inbox", "transcripts", "candidates", "renders", "posts", "posters", "targets", "jobs", "kv"];

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        for t in TABLES {
            conn.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {t} (id TEXT PRIMARY KEY, json TEXT NOT NULL, ord TEXT NOT NULL DEFAULT ''); CREATE INDEX IF NOT EXISTS {t}_ord ON {t}(ord);"))?;
        }
        conn.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(source_id UNINDEXED, text, tokenize = 'unicode61 remove_diacritics 2');")?;
        Ok(Self { conn })
    }

    pub fn put<T: Serialize>(&self, table: &str, id: &str, ord: &str, v: &T) -> rusqlite::Result<()> {
        let json = serde_json::to_string(v).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        self.conn.execute(
            &format!("INSERT INTO {table} (id,json,ord) VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET json=excluded.json, ord=excluded.ord"),
            params![id, json, ord],
        )?;
        Ok(())
    }

    pub fn get<T: DeserializeOwned>(&self, table: &str, id: &str) -> rusqlite::Result<Option<T>> {
        let raw: Option<String> = self.conn.query_row(&format!("SELECT json FROM {table} WHERE id=?1"), params![id], |r| r.get(0)).optional()?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
    }

    /// Oldest first (ord ascending).
    pub fn all<T: DeserializeOwned>(&self, table: &str) -> rusqlite::Result<Vec<T>> {
        let mut st = self.conn.prepare(&format!("SELECT json FROM {table} ORDER BY ord, id"))?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(|s| serde_json::from_str(&s).ok()).collect())
    }

    pub fn delete(&self, table: &str, id: &str) -> rusqlite::Result<()> {
        self.conn.execute(&format!("DELETE FROM {table} WHERE id=?1"), params![id])?;
        Ok(())
    }

    pub fn delete_where<T: DeserializeOwned>(&self, table: &str, pred: impl Fn(&T) -> bool) -> rusqlite::Result<usize> {
        let mut st = self.conn.prepare(&format!("SELECT id, json FROM {table}"))?;
        let rows: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(|r| r.ok()).collect();
        let mut n = 0;
        for (id, json) in rows {
            if serde_json::from_str::<T>(&json).map(|v| pred(&v)).unwrap_or(false) {
                self.delete(table, &id)?;
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn fts_put(&self, source_id: &str, text: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM transcripts_fts WHERE source_id=?1", params![source_id])?;
        self.conn.execute("INSERT INTO transcripts_fts (source_id, text) VALUES (?1, ?2)", params![source_id, text])?;
        Ok(())
    }

    /// Source ids whose transcript matches, best first.
    pub fn fts_search(&self, q: &str) -> rusqlite::Result<Vec<String>> {
        let terms: Vec<String> = q.split_whitespace().map(|t| format!("\"{}\"*", t.replace('"', ""))).collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let mut st = self.conn.prepare("SELECT source_id FROM transcripts_fts WHERE transcripts_fts MATCH ?1 ORDER BY rank LIMIT 100")?;
        let v = st.query_map(params![terms.join(" ")], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(v)
    }

    pub fn settings(&self) -> Settings {
        self.get::<Settings>("kv", "settings").ok().flatten().unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> rusqlite::Result<()> {
        self.put("kv", "settings", "", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    #[test]
    fn roundtrip_and_fts() {
        let db = Db::open_in_memory().unwrap();
        let s = Source { id: "s-1".into(), title: "Sabr".into(), ..Default::default() };
        db.put("sources", &s.id, "2026-01-01", &s).unwrap();
        assert_eq!(db.get::<Source>("sources", "s-1").unwrap().unwrap().title, "Sabr");
        assert_eq!(db.all::<Source>("sources").unwrap().len(), 1);
        db.fts_put("s-1", "sabr is niet wachten, sabr is doorgaan").unwrap();
        assert_eq!(db.fts_search("wacht").unwrap(), vec!["s-1".to_string()]);
        assert!(db.fts_search("zzz").unwrap().is_empty());
        db.delete("sources", "s-1").unwrap();
        assert!(db.all::<Source>("sources").unwrap().is_empty());
        assert_eq!(db.settings().max_reel_s, 40);
    }
}
