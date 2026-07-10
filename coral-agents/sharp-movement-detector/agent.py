#!/usr/bin/env python3
"""sharp-movement-detector — the quantitative specialist.

This is the "sharp money" voice in the debate. When the orchestrator delegates
the *trading* track, this agent turns raw market odds into a probabilistic
value assessment:

  - it reads the best available decimal odds for each 1X2 selection from the
    delegation signal,
  - converts them to implied probabilities (stripping the overround so the
    book's three implied probs sum back to ~1.0),
  - compares its own model probability against the fair market probability,
  - and, when the edge on the best selection clears a threshold, proposes a
    `Wager` with a *suggested* Kelly stake.

The stake it proposes is advisory only. The Rust Authority recomputes the
Kelly fraction, clamps to the devnet cap, and owns every status past
`Debated`. If there is no edge, this specialist returns a `no_bet` narrative
rather than forcing a wager — an honest "no value here" is a valid outcome.

The signal payload shape is defined by `runtime.rs` when it delegates the
trading track; missing/garbled fields degrade to a neutral no-bet reply
rather than crashing the debate.
"""

from __future__ import annotations

import asyncio
import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Dict, Optional, Tuple

# Make the shared `coral_agent` package importable whether this file is run
# from its own directory (CoralOS launches `python agent.py`) or from the repo
# root. We add the parent `coral-agents/` dir to sys.path.
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

# Minimum edge (model_prob − market_implied) before we bother proposing a bet.
# Mirrors the Authority's `min_edge` intent; the Authority still enforces its
# own gate, this just avoids proposing obviously value-less wagers.
MIN_EDGE = float(os.environ.get("SHARP_MIN_EDGE", "0.02"))


class SharpMovementDetector(Specialist):
    """Quantitative value specialist for the trading track."""

    name = "sharp-movement-detector"

    async def handle(self, delegation: Delegation) -> Reply:
        odds = _extract_odds(delegation.signal)
        model = _extract_model_probs(delegation.signal)

        if not odds:
            return Reply(
                ack=(
                    "sharp-movement-detector: no market odds in delegation; "
                    "cannot assess value (no bet)"
                ),
                contribution_kind="analysis",
            )

        # Fair (overround-free) market probabilities per selection.
        fair = _fair_probabilities(odds)

        # Pick the selection with the largest positive edge.
        best: Optional[Tuple[Selection, float, float, float]] = None
        for sel in (Selection.HOME, Selection.DRAW, Selection.AWAY):
            if sel not in odds or sel not in fair:
                continue
            market_implied = fair[sel]
            model_prob = model.get(sel, market_implied)  # default: agree with market
            edge = model_prob - market_implied
            if best is None or edge > best[3]:
                best = (sel, odds[sel], model_prob, edge)

        if best is None:
            return Reply(
                ack="sharp-movement-detector: odds incomplete; no bet",
                contribution_kind="analysis",
            )

        selection, decimal_odds, model_prob, edge = best
        market_implied = fair[selection]

        if edge <= MIN_EDGE:
            return Reply(
                ack=(
                    f"sharp-movement-detector: best selection {selection.value} "
                    f"edge {edge:+.3f} below {MIN_EDGE:.3f} threshold — no value, no bet"
                ),
                contribution_kind="analysis",
            )

        # Suggested Kelly stake (advisory). Authority re-derives + clamps.
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
                f"Model {model_prob:.1%} vs market {market_implied:.1%} on "
                f"{selection.value} @ {decimal_odds:.2f} → +{edge:.1%} edge; "
                f"Kelly {frac:.1%} of bankroll."
            ),
            proof_ref=None,
            status=WagerStatus.PROPOSED,
            debate=None,
            created_at=datetime.now(timezone.utc).isoformat(),
        )

        return Reply(
            ack=(
                f"sharp-movement-detector: VALUE on {selection.value} @ "
                f"{decimal_odds:.2f} — model {model_prob:.1%} vs market "
                f"{market_implied:.1%} (+{edge:.1%}); proposing {suggested_stake} SOL "
                f"(Kelly {frac:.1%}, pending Authority sizing)"
            ),
            wager=wager,
            contribution_kind="signal",
        )


def _extract_odds(signal: Dict) -> Dict[Selection, float]:
    """Pull decimal odds per selection from a delegation signal.

    Accepts either a flat `{home, draw, away}` shape or a nested `odds` object,
    with numeric or string values. Non-parseable entries are skipped.
    """
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


def _extract_model_probs(signal: Dict) -> Dict[Selection, float]:
    """Pull the specialist's own model probabilities if the signal carries them.

    When absent, callers default to the market-implied probability (i.e. no
    disagreement, hence no edge). This keeps the agent honest: it only claims
    value when the delegation actually gives it a differentiated view.
    """
    model = signal.get("modelProbs") or signal.get("model_probs")
    if not isinstance(model, dict):
        return {}
    out: Dict[Selection, float] = {}
    for sel, keys in (
        (Selection.HOME, ("home", "HOME", "1")),
        (Selection.DRAW, ("draw", "DRAW", "X")),
        (Selection.AWAY, ("away", "AWAY", "2")),
    ):
        for key in keys:
            if key in model:
                val = _to_float(model[key])
                if val is not None and 0.0 < val < 1.0:
                    out[sel] = val
                break
    return out


def _fair_probabilities(odds: Dict[Selection, float]) -> Dict[Selection, float]:
    """Normalise implied probabilities to remove the book's overround.

    Raw implied probs sum to >1 (the vig). Dividing each by the sum yields a
    fair probability set that sums to 1.0, which is what we compare the model
    against.
    """
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


if __name__ == "__main__":
    try:
        asyncio.run(SharpMovementDetector().run())
    except KeyboardInterrupt:
        log("sharp-movement-detector", "interrupted; shutting down")
