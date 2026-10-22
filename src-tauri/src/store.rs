use crate::model::*;
use anyhow::Result;
use rusqlite::{params, Connection};
use std::{path::Path, sync::Mutex};
pub struct Store {
    db: Mutex<Connection>,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY, data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS config(key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn tasks(&self) -> Result<Vec<Task>> {
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare("SELECT data FROM tasks ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|x| Ok(serde_json::from_str(&x?)?)).collect()
    }
    pub fn save_task(&self, t: &Task) -> Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO tasks(id,data) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![t.id,serde_json::to_string(t)?])?;
        Ok(())
    }
    pub fn delete_task(&self, id: &str) -> Result<()> {
        self.db
            .lock()
            .unwrap()
            .execute("DELETE FROM tasks WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn settings(&self) -> Result<Settings> {
        let db = self.db.lock().unwrap();
        let r = db.query_row("SELECT value FROM config WHERE key='settings'", [], |r| {
            r.get::<_, String>(0)
        });
        match r {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Settings::default()),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO config(key,value) VALUES('settings',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(s)?])?;
        Ok(())
    }
    pub fn save_parse(&self, p: &ParseState) -> Result<()> {
        self.db.lock().unwrap().execute("INSERT INTO config(key,value) VALUES('parse',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(p)?])?;
        Ok(())
    }
    pub fn load_parse(&self) -> ParseState {
        self.db
            .lock()
            .unwrap()
            .query_row("SELECT value FROM config WHERE key='parse'", [], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
}
