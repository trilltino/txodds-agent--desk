#!/usr/bin/env python3
"""match-intelligence-agent — the fundamentals / model-probability specialist.

Where sharp-movement-detector reads the *market* and fan-pundit-agent reads the
*story*, this agent reads the *fixture*. It is the quantitative fundamentals
voice: it converts structured match context — recent form, expected goals,
injuries/availability, home advantage, league-rank gap, and head-to-head record
— into a fair 1X2 probability distribution `{HOME, DRAW, AWAY}` that sums to 1.

That distribution is the baseline `model_prob` the rest of the debate reasons
about:

  - sharp-movement-detector compares it against the overround-stripped market to
    find value,
  - fan-pundit-agent nudges it with narrative conviction,
  - the Rust Authority ultimately re-derives Kelly sizing and clamps the stake.

If the delegation also carries market odds, this agent will surface the
best-value selection as a *proposed* `Wager` (status `Proposed`) so the trading
track has something concrete to argue over. If no odds are present it emits a
pure `analysis` reply carrying the model distribution in `extra["modelProbs"]`,
which the orchestrator forwards to the sharp specialist.

The scoring model here is deliberately transparent and bounded — a weighted
linear feature score per side fed through a softmax with a fixed draw prior.
It is a *reasoning aid*, not a black box: every input degrades to a neutral
contribution rather than crashing the debate, and the Authority owns every
number that touches money.
"""

from __future__ import annotations

import asyncio
import math
import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Dict, Optional, Tuple

# Make the shared `coral_agent` package importable whether this file is run from
# its own directory (CoralOS launches `python agent.py`) or from the repo root.
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from coral_agent import (  # noqa: E402
    Delegation,
    Reply,
    Selection,
    Specialist,
    Wager,
    WagerStatus,
    implied_probability,
    kelly_fraction,
    log,
)

# Softmax temperature: higher = flatter (more uncertain) distributions, lower =
# sharper. Tuned so a moderate feature-score gap yields a believable favourite
# without collapsing the draw to zero.
MODEL_TEMPERATURE = float(os.environ.get("MI_TEMPERATURE", "1.0"))

# Baseline draw mass. Football draws cluster ~24–28%; we seed the draw with a
# prior and let feature scores pull probability toward the two win outcomes.
DRAW_PRIOR = float(os.environ.get("MI_DRAW_PRIOR", "0.26"))

# Fixed home-field advantage added to the home side's feature score (in the same
# arbitrary "score" units the feature weights produce).
HOME_ADVANTAGE = float(os.environ.get("MI_HOME_ADVANTAGE", "0.35"))

# Minimum edge before we bother attaching a proposed wager. Mirrors the sharp
# specialist / Authority intent; the Authority still enforces its own gate.
MIN_EDGE = float(os.environ.get("MI_MIN_EDGE", "0.02"))

# Feature weights: how strongly each fundamental input moves a side's score.
FORM_WEIGHT = float(os.environ.get("MI_FORM_WEIGHT", "0.30"))
XG_WEIGHT = float(os.environ.get("MI_XG_WEIGHT", "0.45"))
RANK_WEIGHT = float(os.environ.get("MI_RANK_WEIGHT", "0.20"))
INJURY_WEIGHT = float(os.environ.get("MI_INJURY_WEIGHT", "0.15"))
H2H_WEIGHT = float(os.environ.get("MI_H2H_WEIGHT", "0.15"))


class MatchIntelligenceAgent(Specialist):
    """Fundamentals specialist: fixture context → fair 1X2 model probabilities."""

    name = "match-intelligence-agent"

    async def handle(self, delegation: Delegation) -> Reply:
        stats = _extract_stats(delegation.signal)
        model = _model_distribution(stats)

        # The model distribution is this agent's core contribution regardless of
        # whether we can also propose a wager. Carry it in `extra` so downstream
        # specialists (and the Rust orchestrator) can consume it directly.
        model_probs_payload = {
            "home": round(model[Selection.HOME], 6),
            "draw": round(model[Selection.DRAW], 6),
            "away": round(model[Selection.AWAY], 6),
        }
        drivers = _drivers_summary(stats)

        odds = _extract_odds(delegation.signal)
        if not odds:
            # No market to price against — publish the fundamentals view alone so
            # the sharp specialist can pick it up on the trading track.
            return Reply(
                ack=(
                    f"match-intelligence-agent: model {_dist_str(model)} "
                    f"({drivers}); no market odds in delegation, publishing "
                    f"fundamentals for trading track"
                ),
                contribution_kind="analysis",
                extra={"modelProbs": model_probs_payload},
            )

        fair = _fair_probabilities(odds)

        # Choose the selection where our model most exceeds the fair market prob.
        best: Optional[Tuple[Selection, float, float, float]] = None
        for sel in (Selection.HOME, Selection.DRAW, Selection.AWAY):
            if sel not in odds or sel not in fair:
                continue
            market_implied = fair[sel]
            model_prob = model[sel]
            edge = model_prob - market_implied
            if best is None or edge > best[3]:
                best = (sel, odds[sel], model_prob, edge)

        if best is None:
            return Reply(
                ack=(
                    f"match-intelligence-agent: model {_dist_str(model)} "
                    f"({drivers}); market odds incomplete, no wager proposed"
                ),
                contribution_kind="analysis",
                extra={"modelProbs": model_probs_payload},
            )

        selection, decimal_odds, model_prob, edge = best
        market_implied = fair[selection]

        if edge <= MIN_EDGE:
            return Reply(
                ack=(
                    f"match-intelligence-agent: model {_dist_str(model)} "
                    f"({drivers}); best fundamentals edge on {selection.value} "
                    f"is {edge:+.3f}, below {MIN_EDGE:.3f} — fair-priced, no wager"
                ),
                contribution_kind="analysis",
                extra={"modelProbs": model_probs_payload},
            )

        # Advisory Kelly sizing; the Authority re-derives and clamps.
        frac = kelly_fraction(model_prob, decimal_odds)
        bankroll = float(os.environ.get("DEVNET_BANKROLL_SOL", "1.0"))
        suggested_stake = round(frac * bankroll, 6)

        wager = Wager(
            wager_id=str(uuid.uuid4()),
            fixture_id=delegation.fixture_id or 0,
            selection=selection,
            model_prob=round(model_prob, 6),
            market_implied=round(market_implied, 6),
            edge=round(edge, 6),
            fair_odds=round(1.0 / model_prob, 4) if model_prob > 0 else decimal_odds,
            stake_sol=suggested_stake,
            thesis=(
                f"Fundamentals model {model_prob:.1%} vs market "
                f"{market_implied:.1%} on {selection.value} @ {decimal_odds:.2f} "
                f"(+{edge:.1%}); drivers: {drivers}. Kelly {frac:.1%} of bankroll."
            ),
            proof_ref=None,
            status=WagerStatus.PROPOSED,
            debate=None,
            created_at=datetime.now(timezone.utc).isoformat(),
        )

        return Reply(
            ack=(
                f"match-intelligence-agent: model {_dist_str(model)} favours "
                f"{selection.value} @ {decimal_odds:.2f} — model {model_prob:.1%} "
                f"vs market {market_implied:.1%} (+{edge:.1%}); proposing "
                f"{suggested_stake} SOL (Kelly {frac:.1%}, pending Authority sizing)"
            ),
            wager=wager,
            contribution_kind="signal",
            extra={"modelProbs": model_probs_payload},
        )


# -- feature extraction ----------------------------------------------------


class _SideStats:
    """Per-side fundamentals, all optional and defaulting to neutral (0.0)."""

    __slots__ = ("form", "xg", "rank", "injuries")

    def __init__(self) -> None:
        self.form: float = 0.0        # recent points-per-game or rolling rating
        self.xg: float = 0.0          # expected goals for − against (net)
        self.rank: Optional[float] = None  # league position (lower = better)
        self.injuries: float = 0.0    # count of key absentees (>0 hurts the side)


class _MatchStats:
    __slots__ = ("home", "away", "h2h")

    def __init__(self) -> None:
        self.home = _SideStats()
        self.away = _SideStats()
        self.h2h: float = 0.0  # signed: +ve favours home, -ve favours away


def _extract_stats(signal: Dict) -> _MatchStats:
    """Parse fixture fundamentals from a delegation signal, tolerating shapes.

    Accepts either a nested `stats`/`intelligence` object or the flat signal,
    and either `{home:{...}, away:{...}}` sub-objects or flat `homeForm`/
    `awayXg`-style keys. Anything missing stays neutral so a sparse signal still
    yields a sane (near-uniform) distribution rather than an error.
    """
    root = signal.get("stats") or signal.get("intelligence") or signal
    stats = _MatchStats()

    home_obj = root.get("home") if isinstance(root.get("home"), dict) else None
    away_obj = root.get("away") if isinstance(root.get("away"), dict) else None

    _fill_side(stats.home, home_obj, root, prefix="home")
    _fill_side(stats.away, away_obj, root, prefix="away")

    # Head-to-head: accept a signed number, or {home, away} win counts.
    h2h = root.get("h2h") or root.get("headToHead")
    stats.h2h = _parse_h2h(h2h)
    return stats


def _fill_side(side: _SideStats, obj: Optional[Dict], root: Dict, prefix: str) -> None:
    """Populate one side from a sub-object, falling back to prefixed flat keys."""

    def pick(*names: str) -> Optional[float]:
        if obj is not None:
            for n in names:
                if n in obj:
                    v = _to_float(obj[n])
                    if v is not None:
                        return v
        for n in names:
            flat = f"{prefix}{n[:1].upper()}{n[1:]}"
            if flat in root:
                v = _to_float(root[flat])
                if v is not None:
                    return v
            if n in root and prefix in ("home",):  # bare keys map to home only
                v = _to_float(root[n])
                if v is not None:
                    return v
        return None

    form = pick("form", "ppg", "rating")
    if form is not None:
        side.form = form
    xg = pick("xg", "xgDiff", "netXg", "xGDiff")
    if xg is not None:
        side.xg = xg
    rank = pick("rank", "position", "standing")
    if rank is not None:
        side.rank = rank
    injuries = pick("injuries", "absentees", "keyOut")
    if injuries is not None:
        side.injuries = injuries


def _parse_h2h(h2h) -> float:
    if isinstance(h2h, (int, float)):
        return _clamp(float(h2h), -1.0, 1.0)
    if isinstance(h2h, dict):
        home_w = _to_float(h2h.get("home") or h2h.get("homeWins")) or 0.0
        away_w = _to_float(h2h.get("away") or h2h.get("awayWins")) or 0.0
        total = home_w + away_w
        if total <= 0:
            return 0.0
        return _clamp((home_w - away_w) / total, -1.0, 1.0)
    return 0.0


# -- model -----------------------------------------------------------------


def _side_score(side: _SideStats, opp: _SideStats) -> float:
    """Weighted linear fundamentals score for one side relative to the opponent."""
    score = 0.0
    score += FORM_WEIGHT * (side.form - opp.form)
    score += XG_WEIGHT * (side.xg - opp.xg)
    # Rank: lower position number is better, so opponent_rank − side_rank is
    # positive when this side is higher-ranked. Only applied if both known.
    if side.rank is not None and opp.rank is not None:
        score += RANK_WEIGHT * math.tanh((opp.rank - side.rank) / 5.0)
    # Injuries hurt the side carrying them (and help via the opponent's).
    score += INJURY_WEIGHT * (opp.injuries - side.injuries)
    return score


def _model_distribution(stats: _MatchStats) -> Dict[Selection, float]:
    """Produce a fair 1X2 distribution summing to 1.0 from the fundamentals.

    Home/away win scores come from the weighted feature diff (plus home
    advantage and head-to-head). They are softmaxed against a draw seeded by
    `DRAW_PRIOR`, so a perfectly balanced fixture returns roughly
    `{home>away, draw≈prior}` with the home edge coming only from HOME_ADVANTAGE.
    """
    home_score = _side_score(stats.home, stats.away) + HOME_ADVANTAGE + H2H_WEIGHT * stats.h2h
    away_score = _side_score(stats.away, stats.home) - H2H_WEIGHT * stats.h2h

    # Draw logit is derived from the prior and pulled down as the two win scores
    # diverge (mismatches draw less often).
    draw_logit = math.log(max(DRAW_PRIOR, 1e-6) / max(1.0 - DRAW_PRIOR, 1e-6))
    draw_logit -= 0.5 * abs(home_score - away_score)

    logits = {
        Selection.HOME: home_score,
        Selection.DRAW: draw_logit,
        Selection.AWAY: away_score,
    }
    return _softmax(logits, MODEL_TEMPERATURE)


def _softmax(logits: Dict[Selection, float], temperature: float) -> Dict[Selection, float]:
    t = temperature if temperature > 1e-6 else 1.0
    scaled = {k: v / t for k, v in logits.items()}
    m = max(scaled.values())
    exps = {k: math.exp(v - m) for k, v in scaled.items()}
    total = sum(exps.values())
    if total <= 0:
        n = len(logits)
        return {k: 1.0 / n for k in logits}
    return {k: v / total for k, v in exps.items()}


def _drivers_summary(stats: _MatchStats) -> str:
    """One-line human explanation of what moved the model, for the transcript."""
    bits = []
    xg_diff = stats.home.xg - stats.away.xg
    if abs(xg_diff) >= 0.15:
        bits.append(f"xG {'home' if xg_diff > 0 else 'away'}+{abs(xg_diff):.2f}")
    form_diff = stats.home.form - stats.away.form
    if abs(form_diff) >= 0.1:
        bits.append(f"form {'home' if form_diff > 0 else 'away'}")
    if stats.home.rank is not None and stats.away.rank is not None:
        if stats.home.rank < stats.away.rank:
            bits.append("home higher-ranked")
        elif stats.away.rank < stats.home.rank:
            bits.append("away higher-ranked")
    inj = stats.away.injuries - stats.home.injuries
    if abs(inj) >= 1:
        bits.append(f"{'away' if inj > 0 else 'home'} injuries")
    if abs(stats.h2h) >= 0.2:
        bits.append(f"h2h {'home' if stats.h2h > 0 else 'away'}")
    return ", ".join(bits) if bits else "balanced fundamentals"


def _dist_str(model: Dict[Selection, float]) -> str:
    return (
        f"H {model[Selection.HOME]:.0%} / D {model[Selection.DRAW]:.0%} / "
        f"A {model[Selection.AWAY]:.0%}"
    )


# -- odds (shared shape with sharp-movement-detector) ----------------------


def _extract_odds(signal: Dict) -> Dict[Selection, float]:
    """Pull decimal odds per selection; mirrors sharp-movement-detector."""
    source = signal.get("odds") if isinstance(signal.get("odds"), dict) else signal
    out: Dict[Selection, float] = {}
    for sel, keys in (
        (Selection.HOME, ("home", "HOME", "1")),
        (Selection.DRAW, ("draw", "DRAW", "X")),
        (Selection.AWAY, ("away", "AWAY", "2")),
    ):
        for key in keys:
            if key in source:
                val = _to_float(source[key])
                if val is not None and val > 1.0:
                    out[sel] = val
                break
    return out


def _fair_probabilities(odds: Dict[Selection, float]) -> Dict[Selection, float]:
    """Normalise implied probabilities to strip the overround (sums to 1.0)."""
    implied: Dict[Selection, float] = {}
    for sel, dec in odds.items():
        p = implied_probability(dec)
        if p is not None:
            implied[sel] = p
    total = sum(implied.values())
    if total <= 0:
        return {}
    return {sel: p / total for sel, p in implied.items()}


def _to_float(value) -> Optional[float]:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _clamp(value: float, lo: float, hi: float) -> float:
    return max(lo, min(hi, value))


if __name__ == "__main__":
    try:
        asyncio.run(MatchIntelligenceAgent().run())
    except KeyboardInterrupt:
        log("match-intelligence-agent", "interrupted; shutting down")
