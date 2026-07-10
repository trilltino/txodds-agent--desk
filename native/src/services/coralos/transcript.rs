//! Replayable Coral transcript artifacts.
//!
//! Source files stay clean: transcripts are written under the app data replay
//! directory, one folder per run, so judge demos can be reconstructed after the
//! process exits.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::AppError;
use crate::types::{AgentTraceEvent, CoralMessage, TxLineProofReceipt};

const CORAL_TRANSCRIPT: &str = "coral_transcript.jsonl";
const AGENT_TRACE: &str = "agent_trace.jsonl";
const PROOF_RECEIPT: &str = "proof_receipts.json";

pub fn persist_run_artifacts(
    replay_dir: &Path,
    run_id: &str,
    messages: &[CoralMessage],
    trace: &[AgentTraceEvent],
    proof: Option<&TxLineProofReceipt>,
) -> Result<(), AppError> {
    let dir = run_dir(replay_dir, run_id);
    fs::create_dir_all(&dir)?;
    write_jsonl(&dir.join(CORAL_TRANSCRIPT), messages)?;
    write_jsonl(&dir.join(AGENT_TRACE), trace)?;
    if let Some(proof) = proof {
        fs::write(dir.join(PROOF_RECEIPT), serde_json::to_vec_pretty(proof)?)?;
    }
    Ok(())
}

pub fn read_coral_messages(replay_dir: &Path, run_id: &str) -> Result<Vec<CoralMessage>, AppError> {
    read_jsonl(&run_dir(replay_dir, run_id).join(CORAL_TRANSCRIPT))
}

pub fn read_agent_trace(replay_dir: &Path, run_id: &str) -> Result<Vec<AgentTraceEvent>, AppError> {
    read_jsonl(&run_dir(replay_dir, run_id).join(AGENT_TRACE))
}

fn run_dir(replay_dir: &Path, run_id: &str) -> PathBuf {
    replay_dir.join("runs").join(safe_segment(run_id))
}

fn safe_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn write_jsonl<T: Serialize>(path: &Path, values: &[T]) -> Result<(), AppError> {
    let mut output = String::new();
    for value in values {
        output.push_str(&serde_json::to_string(value)?);
        output.push('\n');
    }
    fs::write(path, output)?;
    Ok(())
}

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, AppError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(AppError::Io(err)),
    };
    let mut values = Vec::new();
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        values.push(serde_json::from_str(line)?);
    }
    Ok(values)
}
