//! Environment-driven configuration for the sharp-movement-detector poll loop.

pub(crate) struct Config {
    pub(crate) api_base: String,
    pub(crate) api_key: String,
    pub(crate) poll_interval_secs: u64,
    pub(crate) odds_move_threshold_pct: f64,
    pub(crate) confidence_gate: f64,
    pub(crate) max_steps: u64,
    pub(crate) max_tool_rounds: u32,
    pub(crate) signal_log_path: String,
    /// Base URL + bearer token for the desktop app's loopback diagnostics API
    /// (`native/src/web.rs`), if this process runs on the same host as the
    /// desktop app. Empty when unset — the Get* research tools then fail
    /// fast with a clear "not configured" message rather than erroring the
    /// whole reasoning loop (see `rig_venice::tools::get_desk_json`).
    pub(crate) desk_api_base: String,
    pub(crate) desk_api_token: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, String> {
        // VENICE_API_KEY is not stored here — `rig_venice::client()` reads it
        // directly from the environment at construction time (never held in
        // a struct that could leak into a prompt or log).
        require_env("VENICE_API_KEY")?;
        Ok(Self {
            api_base:                env_or("TXLINE_API_BASE", "https://txline.txodds.com/api/v1"),
            api_key:                 require_env("TXLINE_API_KEY")?,
            poll_interval_secs:      env_parse("POLL_INTERVAL_SECS", 60u64),
            odds_move_threshold_pct: env_parse_f64("ODDS_MOVE_THRESHOLD_PCT", 4.0),
            confidence_gate:         env_parse_f64("CONFIDENCE_GATE", 0.55),
            max_steps:               env_parse("MAX_STEPS", 500u64),
            max_tool_rounds:         env_parse("MAX_TOOL_ROUNDS", 6u32),
            signal_log_path:         env_or("SIGNAL_LOG_PATH", "sharp-signals.jsonl"),
            desk_api_base:           env_or("DESK_API_BASE", ""),
            desk_api_token:          env_or("DESK_API_TOKEN", ""),
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn require_env(key: &str) -> Result<String, String> {
    std::env::var(key).map_err(|_| format!("required env var {key} is not set"))
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_parse_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
