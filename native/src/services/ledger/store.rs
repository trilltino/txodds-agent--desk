//! SQLite run ledger.
//!
//! The ledger persists complete AgentRun JSON so the app can recover demo/proof
//! history across restarts while the schema remains easy to evolve.

use std::path::Path;

use rusqlite::{params, Connection};

use crate::domain::agent::{AgentDecision, AgentSignal};
use crate::domain::arena::{
    AgentLeaderboardRow, ArenaPositionRow, ArenaScoreRow, ArenaSessionRow, SettlementRow,
    SignalRow, ToolCallRow,
};
use crate::error::AppError;
use crate::services::llm::LlmResponse;
use crate::services::solana_pay::SolanaPayIntent;
use crate::types::{AgentRun, TxLineEvent, TxLineProofReceipt};

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

            CREATE TABLE IF NOT EXISTS agent_observations (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                fixture_id INTEGER NOT NULL,
                event_id TEXT NOT NULL,
                event_kind TEXT NOT NULL,
                event_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_signals (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                fixture_id INTEGER NOT NULL,
                signal_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_decisions (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                signal_id TEXT,
                action TEXT NOT NULL,
                decision_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS proof_receipts (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                fixture_id INTEGER NOT NULL,
                seq INTEGER,
                status TEXT NOT NULL,
                verified INTEGER NOT NULL,
                receipt_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_llm_calls (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                used INTEGER NOT NULL,
                response_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            -- ── Arena tables (agent-vs-agent) ────────────────────────────────

            CREATE TABLE IF NOT EXISTS arena_positions (
                position_id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                strategy TEXT NOT NULL,
                fixture_id INTEGER NOT NULL,
                market_key TEXT NOT NULL,
                selection TEXT NOT NULL,
                odds_at_entry REAL NOT NULL,
                odds_move_pct REAL NOT NULL,
                direction TEXT NOT NULL,
                confidence REAL NOT NULL,
                recorded_at TEXT NOT NULL,
                tx_signature TEXT,
                outcome_won INTEGER,
                outcome_pnl REAL,
                outcome_settled_at TEXT
            );

            CREATE TABLE IF NOT EXISTS arena_settlements (
                idempotency_key TEXT PRIMARY KEY,
                fixture_id INTEGER NOT NULL,
                agent_id TEXT NOT NULL,
                strategy TEXT NOT NULL,
                market_key TEXT NOT NULL,
                selection TEXT NOT NULL,
                direction TEXT NOT NULL,
                odds_at_entry REAL NOT NULL,
                result TEXT NOT NULL,
                pnl_units REAL NOT NULL,
                settled_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS arena_signals (
                signal_id TEXT PRIMARY KEY,
                idempotency_key TEXT NOT NULL,
                fixture_id INTEGER NOT NULL,
                fixture_name TEXT NOT NULL,
                market_key TEXT NOT NULL,
                selection TEXT NOT NULL,
                odds_now REAL NOT NULL,
                odds_prev REAL NOT NULL,
                move_pct REAL NOT NULL,
                direction TEXT NOT NULL,
                confidence REAL NOT NULL,
                detected_at TEXT NOT NULL,
                narrative TEXT,
                correct_so_far INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS arena_tool_calls (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                arguments_json TEXT NOT NULL,
                result_json TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL
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

    pub fn insert_agent_observation(
        &self,
        run_id: &str,
        event: &TxLineEvent,
    ) -> Result<(), AppError> {
        self.conn.execute(
            "
            INSERT INTO agent_observations
                (id, run_id, fixture_id, event_id, event_kind, event_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET event_json = excluded.event_json
            ",
            params![
                format!("{run_id}:{}", event.id),
                run_id,
                event.fixture_id,
                event.id,
                format!("{:?}", event.kind),
                serde_json::to_string(event)?,
                event.ts
            ],
        )?;
        Ok(())
    }

    pub fn insert_agent_signal(&self, run_id: &str, signal: &AgentSignal) -> Result<(), AppError> {
        self.conn.execute(
            "
            INSERT INTO agent_signals (id, run_id, fixture_id, signal_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET signal_json = excluded.signal_json
            ",
            params![
                signal.id,
                run_id,
                signal.fixture_id,
                serde_json::to_string(signal)?,
                signal.created_at
            ],
        )?;
        Ok(())
    }

    pub fn insert_agent_decision(
        &self,
        run_id: &str,
        decision: &AgentDecision,
    ) -> Result<(), AppError> {
        self.conn.execute(
            "
            INSERT INTO agent_decisions (id, run_id, signal_id, action, decision_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET decision_json = excluded.decision_json
            ",
            params![
                decision.id,
                run_id,
                decision.signal_id,
                format!("{:?}", decision.action),
                serde_json::to_string(decision)?,
                decision.created_at
            ],
        )?;
        Ok(())
    }

    pub fn insert_proof_receipt(
        &self,
        run_id: &str,
        receipt: &TxLineProofReceipt,
    ) -> Result<(), AppError> {
        self.conn.execute(
            "
            INSERT INTO proof_receipts
                (id, run_id, fixture_id, seq, status, verified, receipt_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET receipt_json = excluded.receipt_json
            ",
            params![
                format!(
                    "{run_id}:{}:{}",
                    receipt.fixture_id,
                    receipt
                        .seq
                        .map(|seq| seq.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ),
                run_id,
                receipt.fixture_id,
                receipt.seq,
                format!("{:?}", receipt.simulation_status),
                receipt.verified,
                serde_json::to_string(receipt)?,
                crate::types::now_iso()
            ],
        )?;
        Ok(())
    }

    pub fn insert_llm_call(&self, run_id: &str, response: &LlmResponse) -> Result<(), AppError> {
        self.conn.execute(
            "
            INSERT INTO agent_llm_calls
                (id, run_id, provider, model, used, response_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                format!("{run_id}:{}", uuid::Uuid::new_v4()),
                run_id,
                response.provider,
                response.model,
                response.used,
                serde_json::to_string(response)?,
                crate::types::now_iso()
            ],
        )?;
        Ok(())
    }

    // ── Arena query helpers ────────────────────────────────────────────────────

    /// List arena positions, newest first. Optional agent filter.
    pub fn list_arena_positions(
        &self,
        agent_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ArenaPositionRow>, AppError> {
        let rows: Vec<ArenaPositionRow> = if let Some(aid) = agent_id {
            let mut stmt = self.conn.prepare(
                "SELECT position_id, agent_id, strategy, fixture_id, market_key, selection,
                        odds_at_entry, odds_move_pct, direction, confidence, recorded_at,
                        tx_signature, outcome_won, outcome_pnl, outcome_settled_at
                 FROM arena_positions
                 WHERE agent_id = ?1
                 ORDER BY recorded_at DESC LIMIT ?2",
            )?;
            let r = stmt
                .query_map(params![aid, limit], Self::map_arena_position)?
                .collect::<Result<Vec<_>, _>>()?;
            r
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT position_id, agent_id, strategy, fixture_id, market_key, selection,
                        odds_at_entry, odds_move_pct, direction, confidence, recorded_at,
                        tx_signature, outcome_won, outcome_pnl, outcome_settled_at
                 FROM arena_positions
                 ORDER BY recorded_at DESC LIMIT ?1",
            )?;
            let r = stmt
                .query_map(params![limit], Self::map_arena_position)?
                .collect::<Result<Vec<_>, _>>()?;
            r
        };
        Ok(rows)
    }

    fn map_arena_position(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArenaPositionRow> {
        Ok(ArenaPositionRow {
            position_id: row.get(0)?,
            agent_id: row.get(1)?,
            strategy: row.get(2)?,
            fixture_id: row.get(3)?,
            market_key: row.get(4)?,
            selection: row.get(5)?,
            odds_at_entry: row.get(6)?,
            odds_move_pct: row.get(7)?,
            direction: row.get(8)?,
            confidence: row.get(9)?,
            recorded_at: row.get(10)?,
            tx_signature: row.get(11)?,
            outcome_won: row.get::<_, Option<i64>>(12)?.map(|v| v != 0),
            outcome_pnl: row.get(13)?,
            outcome_settled_at: row.get(14)?,
        })
    }

    /// List settlement records, newest first. Optional agent/fixture filter.
    pub fn list_settlement_records(
        &self,
        agent_id: Option<&str>,
        fixture_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<SettlementRow>, AppError> {
        // Build query dynamically based on which filters are present.
        let mut conditions = Vec::<String>::new();
        if agent_id.is_some() {
            conditions.push("agent_id = ?2".to_string());
        }
        if fixture_id.is_some() {
            conditions.push(format!(
                "fixture_id = ?{}",
                if agent_id.is_some() { 3 } else { 2 }
            ));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT idempotency_key, fixture_id, agent_id, strategy, market_key, selection,
                    direction, odds_at_entry, result, pnl_units, settled_at
             FROM arena_settlements
             {where_clause}
             ORDER BY settled_at DESC LIMIT ?1"
        );
        let mut stmt = self.conn.prepare(&sql)?;

        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(SettlementRow {
                idempotency_key: row.get(0)?,
                fixture_id: row.get(1)?,
                agent_id: row.get(2)?,
                strategy: row.get(3)?,
                market_key: row.get(4)?,
                selection: row.get(5)?,
                direction: row.get(6)?,
                odds_at_entry: row.get(7)?,
                result: row.get(8)?,
                pnl_units: row.get(9)?,
                settled_at: row.get(10)?,
            })
        };

        let rows: Vec<SettlementRow> = match (agent_id, fixture_id) {
            (Some(a), Some(f)) => stmt
                .query_map(params![limit, a, f], map_row)?
                .collect::<Result<_, _>>()?,
            (Some(a), None) => stmt
                .query_map(params![limit, a], map_row)?
                .collect::<Result<_, _>>()?,
            (None, Some(f)) => stmt
                .query_map(params![limit, f], map_row)?
                .collect::<Result<_, _>>()?,
            (None, None) => stmt
                .query_map(params![limit], map_row)?
                .collect::<Result<_, _>>()?,
        };
        Ok(rows)
    }

    /// List signal records, newest first.
    pub fn list_signal_records(
        &self,
        fixture_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<SignalRow>, AppError> {
        let rows: Vec<SignalRow> = if let Some(fid) = fixture_id {
            let mut stmt = self.conn.prepare(
                "SELECT signal_id, idempotency_key, fixture_id, fixture_name, market_key,
                        selection, odds_now, odds_prev, move_pct, direction, confidence,
                        detected_at, narrative, correct_so_far
                 FROM arena_signals
                 WHERE fixture_id = ?1
                 ORDER BY detected_at DESC LIMIT ?2",
            )?;
            let r = stmt
                .query_map(params![fid, limit], Self::map_signal_row)?
                .collect::<Result<_, _>>()?;
            r
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT signal_id, idempotency_key, fixture_id, fixture_name, market_key,
                        selection, odds_now, odds_prev, move_pct, direction, confidence,
                        detected_at, narrative, correct_so_far
                 FROM arena_signals
                 ORDER BY detected_at DESC LIMIT ?1",
            )?;
            let r = stmt
                .query_map(params![limit], Self::map_signal_row)?
                .collect::<Result<_, _>>()?;
            r
        };
        Ok(rows)
    }

    fn map_signal_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SignalRow> {
        Ok(SignalRow {
            signal_id: row.get(0)?,
            idempotency_key: row.get(1)?,
            fixture_id: row.get(2)?,
            fixture_name: row.get(3)?,
            market_key: row.get(4)?,
            selection: row.get(5)?,
            odds_now: row.get(6)?,
            odds_prev: row.get(7)?,
            move_pct: row.get(8)?,
            direction: row.get(9)?,
            confidence: row.get(10)?,
            detected_at: row.get(11)?,
            narrative: row.get(12)?,
            correct_so_far: row.get::<_, i64>(13)? != 0,
        })
    }

    /// Aggregate arena score across all settled positions.
    pub fn get_arena_score(&self) -> Result<ArenaScoreRow, AppError> {
        // Single query for all aggregates – keep it correct and fast.
        let (follow_wins, follow_losses, fade_wins, fade_losses, follow_pnl, fade_pnl): (
            i64,
            i64,
            i64,
            i64,
            f64,
            f64,
        ) = self.conn.query_row(
            "SELECT
                SUM(CASE WHEN (strategy = 'FollowSharp' OR strategy = 'follow_sharp') AND result = 'win'  THEN 1 ELSE 0 END),
                SUM(CASE WHEN (strategy = 'FollowSharp' OR strategy = 'follow_sharp') AND result = 'loss' THEN 1 ELSE 0 END),
                SUM(CASE WHEN (strategy = 'FadeSharp'  OR strategy = 'fade_sharp')  AND result = 'win'  THEN 1 ELSE 0 END),
                SUM(CASE WHEN (strategy = 'FadeSharp'  OR strategy = 'fade_sharp')  AND result = 'loss' THEN 1 ELSE 0 END),
                SUM(CASE WHEN (strategy = 'FollowSharp' OR strategy = 'follow_sharp') THEN pnl_units ELSE 0.0 END),
                SUM(CASE WHEN (strategy = 'FadeSharp'  OR strategy = 'fade_sharp')  THEN pnl_units ELSE 0.0 END)
             FROM arena_settlements",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        ).unwrap_or((0, 0, 0, 0, 0.0, 0.0));
        let _ = follow_wins; // shadow the earlier unused binding

        let leader = match follow_pnl.partial_cmp(&fade_pnl) {
            Some(std::cmp::Ordering::Greater) => "FOLLOW (match-intelligence)".to_string(),
            Some(std::cmp::Ordering::Less) => "FADE (contrarian)".to_string(),
            _ => "TIE".to_string(),
        };

        Ok(ArenaScoreRow {
            follow_wins,
            follow_losses,
            fade_wins,
            fade_losses,
            follow_pnl,
            fade_pnl,
            leader,
        })
    }

    /// Per-agent leaderboard ordered by total PnL descending.
    pub fn list_agent_leaderboard(&self) -> Result<Vec<AgentLeaderboardRow>, AppError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                agent_id,
                strategy,
                COUNT(*) AS positions_taken,
                SUM(CASE WHEN result = 'win' THEN 1 ELSE 0 END) AS positions_won,
                SUM(pnl_units) AS total_pnl,
                AVG(CASE WHEN result = 'win' THEN odds_at_entry ELSE NULL END) AS avg_win_confidence
             FROM arena_settlements
             GROUP BY agent_id, strategy
             ORDER BY total_pnl DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let taken: i64 = row.get(2)?;
                let won: i64 = row.get(3)?;
                let win_rate = if taken > 0 {
                    won as f64 / taken as f64
                } else {
                    0.0
                };
                Ok(AgentLeaderboardRow {
                    agent_id: row.get(0)?,
                    strategy: row.get(1)?,
                    positions_taken: taken,
                    positions_won: won,
                    total_pnl_points: row.get(4)?,
                    win_rate,
                    avg_winning_confidence: row.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List tool-call audit rows for a given run, oldest first.
    pub fn list_tool_call_records(
        &self,
        run_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ToolCallRow>, AppError> {
        let rows: Vec<ToolCallRow> = if let Some(rid) = run_id {
            let mut stmt = self.conn.prepare(
                "SELECT id, run_id, tool_name, arguments_json, result_json, status, created_at
                 FROM arena_tool_calls
                 WHERE run_id = ?1
                 ORDER BY created_at ASC LIMIT ?2",
            )?;
            let r = stmt
                .query_map(params![rid, limit], Self::map_tool_call_row)?
                .collect::<Result<_, _>>()?;
            r
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, run_id, tool_name, arguments_json, result_json, status, created_at
                 FROM arena_tool_calls
                 ORDER BY created_at DESC LIMIT ?1",
            )?;
            let r = stmt
                .query_map(params![limit], Self::map_tool_call_row)?
                .collect::<Result<_, _>>()?;
            r
        };
        Ok(rows)
    }

    fn map_tool_call_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolCallRow> {
        Ok(ToolCallRow {
            id: row.get(0)?,
            run_id: row.get(1)?,
            tool_name: row.get(2)?,
            arguments_json: row.get(3)?,
            result_json: row.get(4)?,
            status: row.get(5)?,
            created_at: row.get(6)?,
        })
    }

    /// Group arena positions by fixture, returning one `ArenaSessionRow` per
    /// distinct fixture ordered by most-recent activity.  `status` is derived
    /// from the position outcomes: if all positions have outcomes the session is
    /// "settled"; if some do it is "pending_settlement"; otherwise "active".
    pub fn list_arena_sessions(
        &self,
        limit: i64,
    ) -> Result<Vec<ArenaSessionRow>, AppError> {
        // First: find the distinct fixtures that have positions, newest first.
        let mut fixture_stmt = self.conn.prepare(
            "SELECT DISTINCT fixture_id, MIN(recorded_at) AS started_at, MAX(recorded_at) AS last_at
             FROM arena_positions
             GROUP BY fixture_id
             ORDER BY last_at DESC
             LIMIT ?1",
        )?;

        struct FixtureMeta {
            fixture_id: i64,
            started_at: String,
        }

        let fixture_metas: Vec<FixtureMeta> = fixture_stmt
            .query_map(params![limit], |row| {
                Ok(FixtureMeta {
                    fixture_id: row.get(0)?,
                    started_at: row.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;

        let mut sessions = Vec::with_capacity(fixture_metas.len());

        for meta in fixture_metas {
            // Load all positions for this fixture.
            let positions = self.list_arena_positions(None, 1000).map(|all| {
                all.into_iter()
                    .filter(|p| p.fixture_id == meta.fixture_id)
                    .collect::<Vec<_>>()
            })?;

            // Derive status from position outcomes.
            let total = positions.len();
            let settled_count = positions.iter().filter(|p| p.outcome_won.is_some()).count();
            let status = if total == 0 || settled_count == 0 {
                "active"
            } else if settled_count < total {
                "pending_settlement"
            } else {
                "settled"
            };

            // Derive ended_at from the latest settled position.
            let ended_at = positions
                .iter()
                .filter_map(|p| p.outcome_settled_at.as_deref())
                .max()
                .map(|s| s.to_string());

            // Use the fixture_id as a session_id prefix (stable, deterministic).
            let session_id = format!("session:{}", meta.fixture_id);

            // Try to get the fixture name from the arena_signals table.
            let fixture_name: String = self
                .conn
                .query_row(
                    "SELECT fixture_name FROM arena_signals WHERE fixture_id = ?1 LIMIT 1",
                    params![meta.fixture_id],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| format!("Fixture {}", meta.fixture_id));

            sessions.push(ArenaSessionRow {
                session_id,
                fixture_id: meta.fixture_id,
                fixture_name,
                positions,
                status: status.to_string(),
                started_at: meta.started_at,
                ended_at,
            });
        }

        Ok(sessions)
    }
}
