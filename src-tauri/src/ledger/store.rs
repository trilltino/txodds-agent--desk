//! SQLite run ledger.
//!
//! The ledger persists complete AgentRun JSON so the app can recover demo/proof
//! history across restarts while the schema remains easy to evolve.

use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::AppError;
use crate::solana_pay::SolanaPayIntent;
use crate::types::AgentRun;

pub struct LedgerStore {
    // rusqlite connection is synchronous; callers protect LedgerStore with a
    // Mutex when sharing it across async Tauri commands.
    conn: Connection,
}

impl LedgerStore {
    // Open or create the ledger database and ensure required tables exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            -- WAL improves resilience for desktop apps where reads and writes
            -- can happen from separate command/task contexts.
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS runs (
                run_id TEXT PRIMARY KEY,
                track TEXT NOT NULL,
                trigger_json TEXT NOT NULL,
                run_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS payment_intents (
                reference TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                intent_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self { conn })
    }

    // Insert/update a complete run. Storing both trigger_json and run_json keeps
    // future querying options open while preserving the exact UI/audit payload.
    pub fn upsert_run(&self, run: &AgentRun) -> Result<(), AppError> {
        let trigger_json = serde_json::to_string(&run.trigger)?;
        let run_json = serde_json::to_string(run)?;
        // Use the trigger timestamp as created_at when available so list order
        // remains stable after later updates.
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

    // Return the newest runs for the history surface.
    pub fn list_runs(&self) -> Result<Vec<AgentRun>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT run_json FROM runs ORDER BY created_at DESC LIMIT 100")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

        // Deserialize row-by-row so a JSON/schema problem maps through AppError.
        let mut runs = Vec::new();
        for row in rows {
            runs.push(serde_json::from_str::<AgentRun>(&row?)?);
        }
        Ok(runs)
    }

    // Load one persisted run by id.
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

    pub fn upsert_payment_intent(&self, intent: &SolanaPayIntent) -> Result<(), AppError> {
        let intent_json = serde_json::to_string(intent)?;
        self.conn.execute(
            "
            INSERT INTO payment_intents (reference, run_id, intent_json, status, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(reference) DO UPDATE SET
                intent_json = excluded.intent_json,
                status = excluded.status,
                updated_at = excluded.updated_at
            ",
            params![
                &intent.reference,
                &intent.run_id,
                intent_json,
                intent.status_text(),
                &intent.created_at,
                crate::types::now_iso()
            ],
        )?;
        Ok(())
    }

    pub fn list_payment_intents(
        &self,
        run_id: Option<&str>,
    ) -> Result<Vec<SolanaPayIntent>, AppError> {
        let mut intents = Vec::new();
        if let Some(run_id) = run_id {
            let mut stmt = self.conn.prepare(
                "SELECT intent_json FROM payment_intents WHERE run_id = ?1 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![run_id], |row| row.get::<_, String>(0))?;
            for row in rows {
                intents.push(serde_json::from_str::<SolanaPayIntent>(&row?)?);
            }
        } else {
            let mut stmt = self
                .conn
                .prepare("SELECT intent_json FROM payment_intents ORDER BY created_at DESC")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                intents.push(serde_json::from_str::<SolanaPayIntent>(&row?)?);
            }
        }
        Ok(intents)
    }

    pub fn get_payment_intent_by_reference(
        &self,
        reference: &str,
    ) -> Result<SolanaPayIntent, AppError> {
        let intent_json: String = self
            .conn
            .query_row(
                "SELECT intent_json FROM payment_intents WHERE reference = ?1",
                params![reference],
                |row| row.get(0),
            )
            .map_err(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(reference.to_string()),
                other => AppError::Sql(other),
            })?;
        Ok(serde_json::from_str(&intent_json)?)
    }
}
