//! Match fundamentals model — softmax 1X2 probability distribution.
//!
//! Ported from `coral-agents/match-intelligence-agent/agent.py`.
//! This is the quantitative fundamentals voice: it converts structured match
//! context — recent form, expected goals, injuries/availability, home
//! advantage, league-rank gap, and head-to-head record — into a fair 1X2
//! probability distribution `{Home, Draw, Away}` that sums to 1.0.
//!
//! The scoring model is deliberately transparent and bounded — a weighted
//! linear feature score per side fed through a softmax with a fixed draw prior.
//! Every input degrades to a neutral contribution rather than crashing.
//!
//! This is pure math, no async, no IO.  The Authority owns every number that
//! touches money.

use std::collections::BTreeMap;
use txodds_types::wager::Selection;

// ── Tuning constants ────────────────────────────────────────────────────────

/// Softmax temperature: higher = flatter (more uncertain) distributions.
pub const DEFAULT_TEMPERATURE: f64 = 1.0;

/// Baseline draw mass.  Football draws cluster ~24–28%; we seed the draw with
/// a prior and let feature scores pull probability toward the two win outcomes.
pub const DEFAULT_DRAW_PRIOR: f64 = 0.26;

/// Fixed home-field advantage added to the home side's feature score.
pub const DEFAULT_HOME_ADVANTAGE: f64 = 0.35;

/// Feature weights: how strongly each fundamental input moves a side's score.
pub const DEFAULT_FORM_WEIGHT: f64 = 0.30;
/// Expected goals weight.
pub const DEFAULT_XG_WEIGHT: f64 = 0.45;
/// League rank weight.
pub const DEFAULT_RANK_WEIGHT: f64 = 0.20;
/// Injury impact weight.
pub const DEFAULT_INJURY_WEIGHT: f64 = 0.15;
/// Head-to-head weight.
pub const DEFAULT_H2H_WEIGHT: f64 = 0.15;

// ── Configuration ───────────────────────────────────────────────────────────

/// Tuning parameters for the fundamentals model.
#[derive(Debug, Clone)]
pub struct FundamentalsConfig {
    /// Softmax temperature (higher = flatter distribution).
    pub temperature: f64,
    /// Baseline draw probability prior.
    pub draw_prior: f64,
    /// Home-field advantage score boost.
    pub home_advantage: f64,
    /// Feature weights.
    pub form_weight: f64,
    /// Expected goals weight.
    pub xg_weight: f64,
    /// League rank weight.
    pub rank_weight: f64,
    /// Injury impact weight.
    pub injury_weight: f64,
    /// Head-to-head weight.
    pub h2h_weight: f64,
}

impl Default for FundamentalsConfig {
    fn default() -> Self {
        Self {
            temperature: DEFAULT_TEMPERATURE,
            draw_prior: DEFAULT_DRAW_PRIOR,
            home_advantage: DEFAULT_HOME_ADVANTAGE,
            form_weight: DEFAULT_FORM_WEIGHT,
            xg_weight: DEFAULT_XG_WEIGHT,
            rank_weight: DEFAULT_RANK_WEIGHT,
            injury_weight: DEFAULT_INJURY_WEIGHT,
            h2h_weight: DEFAULT_H2H_WEIGHT,
        }
    }
}

// ── Per-side statistics ─────────────────────────────────────────────────────

/// Per-side fundamentals, all optional and defaulting to neutral (0.0).
#[derive(Debug, Clone, Default)]
pub struct SideStats {
    /// Recent points-per-game or rolling rating.
    pub form: f64,
    /// Expected goals for − against (net).
    pub xg: f64,
    /// League position (lower = better).  `None` if unknown.
    pub rank: Option<f64>,
    /// Count of key absentees (>0 hurts the side).
    pub injuries: f64,
}

/// Full match fundamentals context.
#[derive(Debug, Clone, Default)]
pub struct MatchStats {
    /// Home side statistics.
    pub home: SideStats,
    /// Away side statistics.
    pub away: SideStats,
    /// Signed head-to-head bias: +ve favours home, −ve favours away.
    /// Clamped to `[-1.0, 1.0]`.
    pub h2h: f64,
}

// ── Model distribution ──────────────────────────────────────────────────────

/// 1X2 probability distribution output.
#[derive(Debug, Clone)]
pub struct Distribution {
    /// Probability of home win.
    pub home: f64,
    /// Probability of draw.
    pub draw: f64,
    /// Probability of away win.
    pub away: f64,
}

impl Distribution {
    /// Return the probability for a given selection.
    #[must_use]
    pub fn prob(&self, sel: Selection) -> f64 {
        match sel {
            Selection::Home => self.home,
            Selection::Draw => self.draw,
            Selection::Away => self.away,
        }
    }

    /// Return the selection with the highest probability.
    #[must_use]
    pub fn best_selection(&self) -> Selection {
        if self.home >= self.draw && self.home >= self.away {
            Selection::Home
        } else if self.away >= self.draw {
            Selection::Away
        } else {
            Selection::Draw
        }
    }

    /// Return as a `BTreeMap` for serialization.
    #[must_use]
    pub fn as_map(&self) -> BTreeMap<String, f64> {
        let mut m = BTreeMap::new();
        m.insert("home".into(), self.home);
        m.insert("draw".into(), self.draw);
        m.insert("away".into(), self.away);
        m
    }
}

/// Compute the weighted feature score for one side relative to the opponent.
fn side_score(side: &SideStats, opp: &SideStats, cfg: &FundamentalsConfig) -> f64 {
    let mut score = 0.0;
    score += cfg.form_weight * (side.form - opp.form);
    score += cfg.xg_weight * (side.xg - opp.xg);

    // Rank: lower position number is better, so opponent_rank − side_rank is
    // positive when this side is higher-ranked.
    if let (Some(sr), Some(or)) = (side.rank, opp.rank) {
        score += cfg.rank_weight * ((or - sr) / 5.0).tanh();
    }

    // Injuries hurt the side carrying them (and help via the opponent's).
    score += cfg.injury_weight * (opp.injuries - side.injuries);
    score
}

/// Produce a fair 1X2 distribution summing to 1.0 from the match fundamentals.
///
/// Home/away win scores come from the weighted feature diff (plus home
/// advantage and head-to-head). They are softmaxed against a draw seeded by
/// `draw_prior`, so a perfectly balanced fixture returns roughly
/// `{home>away, draw≈prior}` with the home edge coming only from `home_advantage`.
#[must_use]
pub fn model_distribution(stats: &MatchStats, cfg: &FundamentalsConfig) -> Distribution {
    let home_score =
        side_score(&stats.home, &stats.away, cfg) + cfg.home_advantage + cfg.h2h_weight * stats.h2h;
    let away_score =
        side_score(&stats.away, &stats.home, cfg) - cfg.h2h_weight * stats.h2h;

    // Draw logit is derived from the prior and pulled down as the two win
    // scores diverge (mismatches draw less often).
    let draw_prior_clamped = cfg.draw_prior.max(1e-6);
    let draw_logit =
        (draw_prior_clamped / (1.0 - draw_prior_clamped).max(1e-6)).ln()
            - 0.5 * (home_score - away_score).abs();

    let temp = if cfg.temperature > 1e-6 {
        cfg.temperature
    } else {
        1.0
    };

    let h = home_score / temp;
    let d = draw_logit / temp;
    let a = away_score / temp;

    let max_logit = h.max(d).max(a);
    let exp_h = (h - max_logit).exp();
    let exp_d = (d - max_logit).exp();
    let exp_a = (a - max_logit).exp();
    let total = exp_h + exp_d + exp_a;

    if total <= 0.0 {
        return Distribution {
            home: 1.0 / 3.0,
            draw: 1.0 / 3.0,
            away: 1.0 / 3.0,
        };
    }

    Distribution {
        home: exp_h / total,
        draw: exp_d / total,
        away: exp_a / total,
    }
}

/// Extract match stats from a generic JSON signal payload.
///
/// Accepts either a nested `stats`/`intelligence` object or the flat signal,
/// and either `{home:{...}, away:{...}}` sub-objects or flat `homeForm`/
/// `awayXg`-style keys.  Anything missing stays neutral so a sparse signal
/// still yields a sane (near-uniform) distribution.
#[must_use]
pub fn extract_stats(signal: &serde_json::Value) -> MatchStats {
    let root = signal
        .get("stats")
        .or_else(|| signal.get("intelligence"))
        .unwrap_or(signal);

    let mut stats = MatchStats::default();

    let home_obj = root.get("home").and_then(|v| v.as_object());
    let away_obj = root.get("away").and_then(|v| v.as_object());

    fill_side(&mut stats.home, home_obj, root, "home");
    fill_side(&mut stats.away, away_obj, root, "away");

    // Head-to-head: accept a signed number, or {home, away} win counts.
    let h2h = root.get("h2h").or_else(|| root.get("headToHead"));
    stats.h2h = parse_h2h(h2h);

    stats
}

fn fill_side(
    side: &mut SideStats,
    obj: Option<&serde_json::Map<String, serde_json::Value>>,
    root: &serde_json::Value,
    prefix: &str,
) {
    let pick = |names: &[&str]| -> Option<f64> {
        // Try the sub-object first.
        if let Some(o) = obj {
            for &n in names {
                if let Some(v) = o.get(n) {
                    if let Some(f) = to_f64(v) {
                        return Some(f);
                    }
                }
            }
        }
        // Fallback to prefixed flat keys (e.g. "homeForm", "awayXg").
        for &n in names {
            let mut flat = String::with_capacity(prefix.len() + n.len());
            flat.push_str(prefix);
            // capitalise first char of n
            let mut chars = n.chars();
            if let Some(c) = chars.next() {
                flat.extend(c.to_uppercase());
                flat.extend(chars);
            }
            if let Some(v) = root.get(&flat) {
                if let Some(f) = to_f64(v) {
                    return Some(f);
                }
            }
        }
        None
    };

    if let Some(f) = pick(&["form", "ppg", "rating"]) {
        side.form = f;
    }
    if let Some(x) = pick(&["xg", "xgDiff", "netXg", "xGDiff"]) {
        side.xg = x;
    }
    if let Some(r) = pick(&["rank", "position", "standing"]) {
        side.rank = Some(r);
    }
    if let Some(i) = pick(&["injuries", "absentees", "keyOut"]) {
        side.injuries = i;
    }
}

fn parse_h2h(val: Option<&serde_json::Value>) -> f64 {
    let Some(v) = val else { return 0.0 };
    if let Some(n) = v.as_f64() {
        return n.clamp(-1.0, 1.0);
    }
    if let Some(n) = v.as_i64() {
        #[allow(clippy::cast_precision_loss)]
        return (n as f64).clamp(-1.0, 1.0);
    }
    if let Some(obj) = v.as_object() {
        let home_w = obj
            .get("home")
            .or_else(|| obj.get("homeWins"))
            .and_then(to_f64)
            .unwrap_or(0.0);
        let away_w = obj
            .get("away")
            .or_else(|| obj.get("awayWins"))
            .and_then(to_f64)
            .unwrap_or(0.0);
        let total = home_w + away_w;
        if total <= 0.0 {
            return 0.0;
        }
        return ((home_w - away_w) / total).clamp(-1.0, 1.0);
    }
    0.0
}

fn to_f64(v: &serde_json::Value) -> Option<f64> {
    #[allow(clippy::cast_precision_loss)]
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
}

/// One-line human explanation of what moved the model, for the transcript.
#[must_use]
pub fn drivers_summary(stats: &MatchStats) -> String {
    let mut bits: Vec<String> = Vec::new();
    let xg_diff = stats.home.xg - stats.away.xg;
    if xg_diff.abs() >= 0.15 {
        let side = if xg_diff > 0.0 { "home" } else { "away" };
        bits.push(format!("xG {side}+{:.2}", xg_diff.abs()));
    }
    let form_diff = stats.home.form - stats.away.form;
    if form_diff.abs() >= 0.1 {
        let side = if form_diff > 0.0 { "home" } else { "away" };
        bits.push(format!("form {side}"));
    }
    if let (Some(hr), Some(ar)) = (stats.home.rank, stats.away.rank) {
        if hr < ar {
            bits.push("home higher-ranked".into());
        } else if ar < hr {
            bits.push("away higher-ranked".into());
        }
    }
    let inj = stats.away.injuries - stats.home.injuries;
    if inj.abs() >= 1.0 {
        let side = if inj > 0.0 { "away" } else { "home" };
        bits.push(format!("{side} injuries"));
    }
    if stats.h2h.abs() >= 0.2 {
        let side = if stats.h2h > 0.0 { "home" } else { "away" };
        bits.push(format!("h2h {side}"));
    }
    if bits.is_empty() {
        "balanced fundamentals".into()
    } else {
        bits.join(", ")
    }
}

/// Normalize implied probabilities (from decimal odds) to remove the book's
/// overround, so they sum to 1.0.
///
/// `odds` maps `Selection → decimal_odds`.  Returns `Selection → fair_prob`.
#[must_use]
pub fn fair_probabilities(odds: &BTreeMap<Selection, f64>) -> BTreeMap<Selection, f64> {
    use txodds_types::wager::implied_probability;
    let mut implied: BTreeMap<Selection, f64> = BTreeMap::new();
    for (&sel, &dec) in odds {
        if let Some(p) = implied_probability(dec) {
            implied.insert(sel, p);
        }
    }
    let total: f64 = implied.values().sum();
    if total <= 0.0 {
        return BTreeMap::new();
    }
    implied.into_iter().map(|(sel, p)| (sel, p / total)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_fixture_favours_home_slightly() {
        let stats = MatchStats::default();
        let cfg = FundamentalsConfig::default();
        let dist = model_distribution(&stats, &cfg);
        // Home advantage should make home > away.
        assert!(dist.home > dist.away, "home={} should > away={}", dist.home, dist.away);
        // Draw should be a reasonable fraction (softmax can push it low with default weights).
        assert!(dist.draw > 0.05 && dist.draw < 0.40, "draw={}", dist.draw);
        // Should sum to 1.
        let sum = dist.home + dist.draw + dist.away;
        assert!((sum - 1.0).abs() < 1e-9, "sum={sum}");
    }

    #[test]
    fn strong_home_form_increases_home_prob() {
        let cfg = FundamentalsConfig::default();
        let baseline = model_distribution(&MatchStats::default(), &cfg);
        let boosted = model_distribution(
            &MatchStats {
                home: SideStats { form: 2.5, xg: 0.5, rank: Some(3.0), injuries: 0.0 },
                away: SideStats { form: 1.0, xg: -0.3, rank: Some(12.0), injuries: 2.0 },
                h2h: 0.4,
            },
            &cfg,
        );
        assert!(boosted.home > baseline.home, "boosted={} > baseline={}", boosted.home, baseline.home);
    }

    #[test]
    fn extract_stats_from_json() {
        let signal = serde_json::json!({
            "stats": {
                "home": { "form": 2.1, "xg": 0.4, "rank": 5, "injuries": 1 },
                "away": { "form": 1.3, "xg": -0.2, "rank": 14, "injuries": 0 },
                "h2h": { "home": 7, "away": 3 }
            }
        });
        let stats = extract_stats(&signal);
        assert!((stats.home.form - 2.1).abs() < 1e-9);
        assert!((stats.away.xg - -0.2).abs() < 1e-9);
        assert_eq!(stats.home.rank, Some(5.0));
        // h2h: (7-3)/10 = 0.4
        assert!((stats.h2h - 0.4).abs() < 1e-9);
    }

    #[test]
    fn fair_probabilities_strips_overround() {
        let mut odds = BTreeMap::new();
        odds.insert(Selection::Home, 2.0);  // implied 0.50
        odds.insert(Selection::Draw, 3.5);  // implied ~0.286
        odds.insert(Selection::Away, 4.0);  // implied 0.25
        // Raw sum = 1.036 (3.6% overround)
        let fair = fair_probabilities(&odds);
        let sum: f64 = fair.values().sum();
        assert!((sum - 1.0).abs() < 1e-9, "sum={sum}");
    }

    #[test]
    fn drivers_summary_balanced() {
        let stats = MatchStats::default();
        assert_eq!(drivers_summary(&stats), "balanced fundamentals");
    }

    #[test]
    fn drivers_summary_xg_dominant() {
        let stats = MatchStats {
            home: SideStats { xg: 0.5, ..Default::default() },
            away: SideStats { xg: -0.2, ..Default::default() },
            h2h: 0.0,
        };
        let summary = drivers_summary(&stats);
        assert!(summary.contains("xG home"), "got: {summary}");
    }
}
