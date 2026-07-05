use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::types::AgentRun;

pub struct LedgerStore {
    conn: Connection,
}

impl LedgerStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS runs (
                run_id TEXT PRIMARY KEY,
                track TEXT NOT NULL,
                trigger_json TEXT NOT NULL,
                run_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self { conn })
    }

    pub fn upsert_run(&self, run: &AgentRun) -> Result<(), AppError> {
        let trigger_json = serde_json::to_string(&run.trigger)?;
        let run_json = serde_json::to_string(run)?;
        let created_at = run
            .timeline
            .first()
            .map(|entry| entry.at.clone())
            .unwrap_or_else(crate::types::now_iso);

        self.conn.execute(
            "
            INSERT INTO runs (run_id, track, trigger_json, run_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(run_id) DO UPDATE SET
                track = excluded.track,
                trigger_json = excluded.trigger_json,
                run_json = excluded.run_json
            ",
            params![
                run.run_id,
                run.track.to_string(),
                trigger_json,
                run_json,
                created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_runs(&self) -> Result<Vec<AgentRun>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT run_json FROM runs ORDER BY created_at DESC LIMIT 100")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

        let mut runs = Vec::new();
        for row in rows {
            runs.push(serde_json::from_str::<AgentRun>(&row?)?);
        }
        Ok(runs)
    }

    pub fn get_run(&self, run_id: &str) -> Result<AgentRun, AppError> {
        let run_json: String = self
            .conn
            .query_row(
                "SELECT run_json FROM runs WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .map_err(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(run_id.to_string()),
                other => AppError::Sql(other),
            })?;
        Ok(serde_json::from_str(&run_json)?)
    }
}
