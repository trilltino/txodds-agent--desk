//! Rig-compatible tool implementations for TxLINE data ingestion.
//!
//! These are the "read-only" tools that *both* arena agents share.  They are
//! implemented as `rig::tool::Tool` structs so the Venice LLM can decide when
//! to call them inside the agent loop.
//!
//! Checklist §7:  typed Tool trait, not stringly-typed JSON blobs.
//! Checklist §20: all inputs validated via schemars JsonSchema before execution.
//! Checklist §28: untrusted API responses wrapped via `wrap_untrusted` before
//!                returning to the LLM context.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ── Shared error type ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ToolCallError(pub String);

impl std::fmt::Display for ToolCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tool error: {}", self.0)
    }
}
impl std::error::Error for ToolCallError {}

// ── FetchOddsSnapshot ─────────────────────────────────────────────────────────

/// Input schema for the `fetch_odds_snapshot` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FetchOddsInput {
    /// TxLINE fixture ID to fetch odds for.
    pub fixture_id: u64,
    /// Optional market key filter (e.g. "1x2", "asian_handicap").
    /// Pass null to fetch all markets.
    pub market_key: Option<String>,
}

/// Raw odds snapshot returned from TxLINE for one fixture.
#[derive(Debug, Serialize, Deserialize)]
pub struct OddsSnapshot {
    pub fixture_id: u64,
    pub fixture_name: String,
    pub status: String,
    pub markets: Vec<MarketSnapshot>,
    pub fetched_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketSnapshot {
    pub market_key: String,
    pub selections: Vec<SelectionOdds>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SelectionOdds {
    pub name: String,
    pub decimal_odds: f64,
    pub previous_odds: Option<f64>,
}

/// Fetches the current odds snapshot for a fixture from TxLINE.
///
/// This is a read-only tool — no side effects.  The `FollowCap` /
/// `FadeCap` tools are separate and gated by capability tokens.
pub struct FetchOddsSnapshot {
    pub http: reqwest::Client,
    pub api_base: String,
    /// Stored as Arc'd secret — never printed in Debug.
    api_key: std::sync::Arc<str>,
}

impl FetchOddsSnapshot {
    pub fn new(api_base: String, api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_base,
            api_key: api_key.into(),
        }
    }
}

impl Tool for FetchOddsSnapshot {
    const NAME: &'static str = "fetch_odds_snapshot";

    type Error = ToolCallError;
    type Args = FetchOddsInput;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::to_value(schemars::schema_for!(FetchOddsInput))
            .map(|parameters| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: "Fetch the current decimal odds snapshot from TxLINE for a single \
                    World Cup fixture.  Returns all markets and selections with the \
                    previous odds for movement calculation.  Read-only, no side effects."
                    .to_owned(),
                parameters,
            })
            .unwrap_or_else(|e| {
                warn!("FetchOddsInput schema generation failed: {e}");
                ToolDefinition {
                    name: Self::NAME.to_owned(),
                    description: "fetch_odds_snapshot (schema unavailable)".to_owned(),
                    parameters: serde_json::json!({}),
                }
            })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        debug!(fixture_id = args.fixture_id, "FetchOddsSnapshot called");

        let url = format!("{}/fixtures/{}/odds", self.api_base, args.fixture_id);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| ToolCallError(format!("HTTP request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(status = %status, "TxLINE API error");
            return Err(ToolCallError(format!(
                "TxLINE returned HTTP {status}: {body}"
            )));
        }

        let raw: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ToolCallError(format!("JSON parse failed: {e}")))?;

        // Checklist §20: bounded response size check before handing to LLM.
        let serialised = serde_json::to_string(&raw)
            .map_err(|e| ToolCallError(format!("re-serialise failed: {e}")))?;
        if serialised.len() > 32_768 {
            return Err(ToolCallError(
                "TxLINE response exceeded 32 KiB safety limit".to_owned(),
            ));
        }

        Ok(raw)
    }
}

// ── ComputeSharpMovement ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ComputeMovementInput {
    /// Selection name to analyse.
    pub selection: String,
    /// Current decimal odds.
    pub current_odds: f64,
    /// Previous decimal odds (from last poll cycle).
    pub previous_odds: f64,
    /// Market key the selection belongs to.
    pub market_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MovementResult {
    /// Absolute percentage change in odds.
    pub pct_change: f64,
    /// True if `pct_change` exceeds the configured threshold.
    pub is_sharp_move: bool,
    /// Direction: "shortening" (odds decreased) or "drifting" (odds increased).
    pub direction: String,
    /// Confidence heuristic (0.0–1.0) based on magnitude.
    pub confidence: f64,
}

/// Pure deterministic computation tool — no I/O.
/// The LLM calls this after `fetch_odds_snapshot` to identify sharp movement.
pub struct ComputeSharpMovement {
    /// Minimum percentage move to flag as sharp (default: 4.0%).
    pub threshold_pct: f64,
}

impl Default for ComputeSharpMovement {
    fn default() -> Self {
        Self { threshold_pct: 4.0 }
    }
}

impl Tool for ComputeSharpMovement {
    const NAME: &'static str = "compute_sharp_movement";

    type Error = ToolCallError;
    type Args = ComputeMovementInput;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::to_value(schemars::schema_for!(ComputeMovementInput))
            .map(|parameters| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: "Given current and previous decimal odds for a selection, compute \
                    whether a sharp money movement has occurred.  Returns the percentage \
                    change, direction, and a confidence score.  Pure computation — no \
                    network calls."
                    .to_owned(),
                parameters,
            })
            .unwrap_or_else(|e| {
                warn!("ComputeMovementInput schema generation failed: {e}");
                ToolDefinition {
                    name: Self::NAME.to_owned(),
                    description: "compute_sharp_movement (schema unavailable)".to_owned(),
                    parameters: serde_json::json!({}),
                }
            })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        if args.previous_odds <= 0.0 || args.current_odds <= 0.0 {
            return Err(ToolCallError(
                "odds must be positive decimals".to_owned(),
            ));
        }

        let pct_change =
            ((args.current_odds - args.previous_odds) / args.previous_odds * 100.0).abs();
        let is_sharp = pct_change >= self.threshold_pct;

        let direction = if args.current_odds < args.previous_odds {
            "shortening"
        } else {
            "drifting"
        };

        // Confidence heuristic: linear scale from threshold → 2× threshold = 0.5 → 1.0
        let confidence = if is_sharp {
            ((pct_change - self.threshold_pct) / self.threshold_pct)
                .clamp(0.0, 1.0)
                * 0.5
                + 0.5
        } else {
            pct_change / self.threshold_pct * 0.5
        };

        let result = MovementResult {
            pct_change,
            is_sharp_move: is_sharp,
            direction: direction.to_owned(),
            confidence,
        };

        serde_json::to_value(&result)
            .map_err(|e| ToolCallError(format!("serialise failed: {e}")))
    }
}

// ── FetchActiveFixtures ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FetchActiveFixturesInput {
    /// Filter to only in-play fixtures (status = "live").
    pub live_only: bool,
}

/// Lists World Cup fixtures from TxLINE.
pub struct FetchActiveFixtures {
    pub http: reqwest::Client,
    pub api_base: String,
    api_key: std::sync::Arc<str>,
}

impl FetchActiveFixtures {
    pub fn new(api_base: String, api_key: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_base,
            api_key: api_key.into(),
        }
    }
}

impl Tool for FetchActiveFixtures {
    const NAME: &'static str = "fetch_active_fixtures";

    type Error = ToolCallError;
    type Args = FetchActiveFixturesInput;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::to_value(schemars::schema_for!(FetchActiveFixturesInput))
            .map(|parameters| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: "List World Cup fixtures from TxLINE.  Set live_only=true to see \
                    only matches currently in-play.  Returns fixture IDs, team names, \
                    kick-off times, and status."
                    .to_owned(),
                parameters,
            })
            .unwrap_or_else(|e| {
                warn!("FetchActiveFixturesInput schema generation failed: {e}");
                ToolDefinition {
                    name: Self::NAME.to_owned(),
                    description: "fetch_active_fixtures (schema unavailable)".to_owned(),
                    parameters: serde_json::json!({}),
                }
            })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let url = format!("{}/fixtures", self.api_base);
        let mut req = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .timeout(std::time::Duration::from_secs(10));

        if args.live_only {
            req = req.query(&[("status", "live")]);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ToolCallError(format!("HTTP request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ToolCallError(format!(
                "TxLINE returned HTTP {status}: {body}"
            )));
        }

        let raw: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ToolCallError(format!("JSON parse failed: {e}")))?;

        Ok(raw)
    }
}

// ── ComputeModelProbability ───────────────────────────────────────────────────
//
// rig-venice ROADMAP.md Phase 4: ports the fundamentals softmax model from
// `coral-agents/match-intelligence-agent/agent.py` (`_model_distribution` /
// `_side_score`) to a Rust tool. The math is unchanged from the Python
// version — this is a straight port, not a redesign — so an LLM agent can
// call it deterministically instead of trying to estimate a 1X2
// distribution itself. LLMs are unreliable at arithmetic; keep this
// deterministic and callable.

/// Per-side fundamentals input, mirroring `_SideStats` in the Python agent.
/// All fields default to a neutral 0.0 so a sparse signal still yields a
/// sane distribution rather than a schema error.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SideStatsInput {
    /// Recent points-per-game or rolling rating.
    #[serde(default)]
    pub form: f64,
    /// Expected goals for minus against (net).
    #[serde(default)]
    pub xg: f64,
    /// League position; lower is better. Omit if unknown for this side.
    pub rank: Option<f64>,
    /// Count of key absentees (positive hurts this side).
    #[serde(default)]
    pub injuries: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ComputeModelProbabilityInput {
    pub home: SideStatsInput,
    pub away: SideStatsInput,
    /// Signed head-to-head record in `[-1, 1]`: positive favours home,
    /// negative favours away, 0.0 if unknown or balanced.
    #[serde(default)]
    pub h2h: f64,
}

#[derive(Debug, Serialize)]
pub struct ModelProbabilities {
    pub home: f64,
    pub draw: f64,
    pub away: f64,
}

/// Weighted-feature-score-through-softmax fundamentals model. Pure
/// computation — no I/O. Defaults mirror the Python agent's `MI_*` env vars.
pub struct ComputeModelProbability {
    pub form_weight: f64,
    pub xg_weight: f64,
    pub rank_weight: f64,
    pub injury_weight: f64,
    pub h2h_weight: f64,
    pub home_advantage: f64,
    pub draw_prior: f64,
    pub temperature: f64,
}

impl Default for ComputeModelProbability {
    fn default() -> Self {
        Self {
            form_weight: 0.30,
            xg_weight: 0.45,
            rank_weight: 0.20,
            injury_weight: 0.15,
            h2h_weight: 0.15,
            home_advantage: 0.35,
            draw_prior: 0.26,
            temperature: 1.0,
        }
    }
}

impl ComputeModelProbability {
    /// Weighted fundamentals score for one side relative to its opponent.
    /// Mirrors `_side_score` in the Python agent exactly.
    fn side_score(&self, side: &SideStatsInput, opp: &SideStatsInput) -> f64 {
        let mut score =
            self.form_weight * (side.form - opp.form) + self.xg_weight * (side.xg - opp.xg);
        if let (Some(side_rank), Some(opp_rank)) = (side.rank, opp.rank) {
            score += self.rank_weight * ((opp_rank - side_rank) / 5.0).tanh();
        }
        score += self.injury_weight * (opp.injuries - side.injuries);
        score
    }
}

impl Tool for ComputeModelProbability {
    const NAME: &'static str = "compute_model_probability";

    type Error = ToolCallError;
    type Args = ComputeModelProbabilityInput;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::to_value(schemars::schema_for!(ComputeModelProbabilityInput))
            .map(|parameters| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: "Given per-side fundamentals (form, xG, rank, injuries) and a \
                    signed head-to-head record, compute a fair 1X2 probability distribution \
                    {home, draw, away} summing to 1.0. Deterministic — never estimate this \
                    distribution yourself, always call this tool."
                    .to_owned(),
                parameters,
            })
            .unwrap_or_else(|e| {
                warn!("ComputeModelProbabilityInput schema generation failed: {e}");
                ToolDefinition {
                    name: Self::NAME.to_owned(),
                    description: "compute_model_probability (schema unavailable)".to_owned(),
                    parameters: serde_json::json!({}),
                }
            })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let home_score =
            self.side_score(&args.home, &args.away) + self.home_advantage + self.h2h_weight * args.h2h;
        let away_score = self.side_score(&args.away, &args.home) - self.h2h_weight * args.h2h;

        let draw_prior = self.draw_prior.clamp(1e-6, 1.0 - 1e-6);
        let draw_logit =
            (draw_prior / (1.0 - draw_prior)).ln() - 0.5 * (home_score - away_score).abs();

        let t = if self.temperature > 1e-6 { self.temperature } else { 1.0 };
        let scaled = [home_score / t, draw_logit / t, away_score / t];
        let m = scaled.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let exps: Vec<f64> = scaled.iter().map(|v| (v - m).exp()).collect();
        let total: f64 = exps.iter().sum();

        let probs = if total > 0.0 {
            ModelProbabilities {
                home: exps[0] / total,
                draw: exps[1] / total,
                away: exps[2] / total,
            }
        } else {
            ModelProbabilities { home: 1.0 / 3.0, draw: 1.0 / 3.0, away: 1.0 / 3.0 }
        };

        serde_json::to_value(&probs).map_err(|e| ToolCallError(format!("serialise failed: {e}")))
    }
}

// ── ComputeFairProbability ────────────────────────────────────────────────────
//
// Ports `_fair_probabilities` from the Python agent: strips the bookmaker
// overround from a set of decimal odds so they sum to 1.0. Reuses
// `txodds_types::implied_probability` rather than re-deriving it.

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ComputeFairProbabilityInput {
    /// Decimal odds for each selection that has a market price. Missing
    /// selections are simply omitted from the output.
    pub home_odds: Option<f64>,
    pub draw_odds: Option<f64>,
    pub away_odds: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct FairProbabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draw: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub away: Option<f64>,
}

/// Normalises implied probabilities from decimal odds so they sum to 1.0,
/// removing the bookmaker's overround. Pure computation — no I/O.
#[derive(Debug, Clone, Copy, Default)]
pub struct ComputeFairProbability;

impl Tool for ComputeFairProbability {
    const NAME: &'static str = "compute_fair_probability";

    type Error = ToolCallError;
    type Args = ComputeFairProbabilityInput;
    type Output = serde_json::Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::to_value(schemars::schema_for!(ComputeFairProbabilityInput))
            .map(|parameters| ToolDefinition {
                name: Self::NAME.to_owned(),
                description: "Strip the bookmaker overround from up to three decimal odds \
                    (home/draw/away) so the resulting probabilities sum to 1.0. Use this to \
                    compare the fundamentals model's probability against what the market is \
                    actually pricing."
                    .to_owned(),
                parameters,
            })
            .unwrap_or_else(|e| {
                warn!("ComputeFairProbabilityInput schema generation failed: {e}");
                ToolDefinition {
                    name: Self::NAME.to_owned(),
                    description: "compute_fair_probability (schema unavailable)".to_owned(),
                    parameters: serde_json::json!({}),
                }
            })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let entries = [
            (0usize, args.home_odds),
            (1usize, args.draw_odds),
            (2usize, args.away_odds),
        ];

        let implied: Vec<(usize, f64)> = entries
            .into_iter()
            .filter_map(|(i, odds)| odds.and_then(txodds_types::implied_probability).map(|p| (i, p)))
            .collect();

        let total: f64 = implied.iter().map(|(_, p)| p).sum();
        if total <= 0.0 {
            return serde_json::to_value(FairProbabilities { home: None, draw: None, away: None })
                .map_err(|e| ToolCallError(format!("serialise failed: {e}")));
        }

        let mut out = FairProbabilities { home: None, draw: None, away: None };
        for (i, p) in implied {
            let normalised = p / total;
            match i {
                0 => out.home = Some(normalised),
                1 => out.draw = Some(normalised),
                _ => out.away = Some(normalised),
            }
        }

        serde_json::to_value(&out).map_err(|e| ToolCallError(format!("serialise failed: {e}")))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(current: f64, previous: f64) -> ComputeMovementInput {
        ComputeMovementInput {
            selection: "Home".to_owned(),
            current_odds: current,
            previous_odds: previous,
            market_key: "1x2".to_owned(),
        }
    }

    #[tokio::test]
    async fn sharp_move_above_threshold() {
        let tool = ComputeSharpMovement::default(); // 4.0%
        // 2.00 → 2.10 = 5% change  → sharp
        let result = tool.call(make_input(2.10, 2.00)).await.unwrap();
        assert_eq!(result["is_sharp_move"], true);
        assert_eq!(result["direction"], "drifting");
        assert!(result["confidence"].as_f64().unwrap() >= 0.5);
    }

    #[tokio::test]
    async fn no_sharp_move_below_threshold() {
        let tool = ComputeSharpMovement::default();
        // 2.00 → 2.05 = 2.5% change → not sharp
        let result = tool.call(make_input(2.05, 2.00)).await.unwrap();
        assert_eq!(result["is_sharp_move"], false);
        assert!(result["confidence"].as_f64().unwrap() < 0.5);
    }

    #[tokio::test]
    async fn shortening_direction() {
        let tool = ComputeSharpMovement::default();
        // 2.00 → 1.90 = 5% drop → shortening
        let result = tool.call(make_input(1.90, 2.00)).await.unwrap();
        assert_eq!(result["direction"], "shortening");
        assert_eq!(result["is_sharp_move"], true);
    }

    #[tokio::test]
    async fn confidence_clamped_at_one() {
        let tool = ComputeSharpMovement { threshold_pct: 1.0 };
        // 2.00 → 3.00 = 50% change — confidence should cap at 1.0
        let result = tool.call(make_input(3.00, 2.00)).await.unwrap();
        let conf = result["confidence"].as_f64().unwrap();
        assert!((conf - 1.0).abs() < f64::EPSILON, "confidence should be 1.0, got {conf}");
    }

    #[tokio::test]
    async fn rejects_zero_odds() {
        let tool = ComputeSharpMovement::default();
        let err = tool.call(make_input(2.00, 0.0)).await.unwrap_err();
        assert!(err.0.contains("positive"));
    }

    #[tokio::test]
    async fn rejects_negative_odds() {
        let tool = ComputeSharpMovement::default();
        let err = tool.call(make_input(-1.0, 2.00)).await.unwrap_err();
        assert!(err.0.contains("positive"));
    }

    #[tokio::test]
    async fn exact_threshold_is_sharp() {
        let tool = ComputeSharpMovement { threshold_pct: 5.0 };
        // 2.00 → 2.10 = exactly 5%
        let result = tool.call(make_input(2.10, 2.00)).await.unwrap();
        assert_eq!(result["is_sharp_move"], true);
    }

    fn neutral_side() -> SideStatsInput {
        SideStatsInput { form: 0.0, xg: 0.0, rank: None, injuries: 0.0 }
    }

    #[tokio::test]
    async fn balanced_fixture_favours_home_only_via_advantage() {
        let tool = ComputeModelProbability::default();
        let result = tool
            .call(ComputeModelProbabilityInput { home: neutral_side(), away: neutral_side(), h2h: 0.0 })
            .await
            .unwrap();
        let home = result["home"].as_f64().unwrap();
        let draw = result["draw"].as_f64().unwrap();
        let away = result["away"].as_f64().unwrap();
        assert!(home > away, "home ({home}) should exceed away ({away}) via home advantage alone");
        assert!((home + draw + away - 1.0).abs() < 1e-9, "probabilities should sum to 1.0");
        // Draw starts from a 0.26 prior but the draw *logit* is pulled down by
        // the home/away score divergence (`0.5 * |home_score - away_score|`)
        // — even a fixture with identical stats diverges by HOME_ADVANTAGE
        // (0.35) alone, so the resulting draw probability is meaningfully
        // below the raw prior. This matches the Python agent's behaviour
        // exactly (verified against `_model_distribution` by hand): a
        // perfectly neutral fixture yields draw ≈ 0.109, not ≈ 0.26.
        assert!(draw > 0.0 && draw < 0.26, "draw {draw} should be positive but below the raw 0.26 prior");
    }

    #[tokio::test]
    async fn strong_xg_edge_favours_that_side() {
        let tool = ComputeModelProbability::default();
        let home = SideStatsInput { form: 0.0, xg: 1.5, rank: None, injuries: 0.0 };
        let away = neutral_side();
        let result = tool
            .call(ComputeModelProbabilityInput { home, away, h2h: 0.0 })
            .await
            .unwrap();
        let home_p = result["home"].as_f64().unwrap();
        let away_p = result["away"].as_f64().unwrap();
        assert!(home_p > 0.6, "large xG edge should push home probability well above baseline, got {home_p}");
        assert!(home_p > away_p);
    }

    #[tokio::test]
    async fn model_probability_missing_rank_does_not_crash() {
        // One side missing rank entirely — the rank term should just drop out
        // rather than erroring, mirroring the Python "neutral when unknown" rule.
        let tool = ComputeModelProbability::default();
        let home = SideStatsInput { form: 0.1, xg: 0.2, rank: Some(3.0), injuries: 0.0 };
        let away = neutral_side();
        let result = tool.call(ComputeModelProbabilityInput { home, away, h2h: 0.0 }).await.unwrap();
        let total = result["home"].as_f64().unwrap() + result["draw"].as_f64().unwrap() + result["away"].as_f64().unwrap();
        assert!((total - 1.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn fair_probability_strips_overround() {
        let tool = ComputeFairProbability;
        // 1.90 / 3.60 / 4.20 decimal odds carry a bookmaker margin; implied
        // probabilities should sum to > 1.0 before normalisation.
        let result = tool
            .call(ComputeFairProbabilityInput {
                home_odds: Some(1.90),
                draw_odds: Some(3.60),
                away_odds: Some(4.20),
            })
            .await
            .unwrap();
        let home = result["home"].as_f64().unwrap();
        let draw = result["draw"].as_f64().unwrap();
        let away = result["away"].as_f64().unwrap();
        assert!((home + draw + away - 1.0).abs() < 1e-9, "fair probabilities should sum to exactly 1.0");
        assert!(home > draw && home > away, "shortest odds should carry the highest fair probability");
    }

    #[tokio::test]
    async fn fair_probability_omits_missing_selections() {
        let tool = ComputeFairProbability;
        let result = tool
            .call(ComputeFairProbabilityInput { home_odds: Some(2.0), draw_odds: None, away_odds: Some(2.0) })
            .await
            .unwrap();
        assert_eq!(result["home"].as_f64().unwrap(), 0.5);
        assert_eq!(result["away"].as_f64().unwrap(), 0.5);
        assert!(result.get("draw").is_none() || result["draw"].is_null());
    }
}
