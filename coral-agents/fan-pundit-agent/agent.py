#!/usr/bin/env python3
"""fan-pundit-agent — the narrative / contrarian specialist.

Where sharp-movement-detector speaks in probabilities, this agent speaks in
*story*: form, motivation, injuries, home crowd, "trap game" dynamics — the
qualitative context that pure market maths misses. In the debate it plays the
adversary that either **endorses** a proposed wager (adding conviction) or
**challenges** it (flagging narrative risk the numbers ignore).

It does NOT originate wagers from thin air. It reads whatever quantitative
proposal is present in the delegation (the sharp signal / decision), then:

  - if the narrative agrees with the value side → `endorse`, nudging model
    confidence up slightly,
  - if the narrative contradicts it (e.g. backing the favourite in a classic
    letdown spot) → `challenge`, nudging confidence down and, when the risk is
    severe, recommending `no_bet`.

Any confidence adjustment is advisory: the Rust Authority still owns the final
probability blend, Kelly sizing, and the proof gate. This agent's job is to
make the debate genuinely adversarial rather than a rubber stamp.
"""

from __future__ import annotations

import asyncio
import os
import sys
from typing import Dict, Optional

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from coral_agent import (  # noqa: E402
    Delegation,
    Reply,
    Selection,
    Specialist,
    Wager,
    WagerStatus,
    log,
)

# How strongly narrative sentiment is allowed to nudge the model probability.
# Deliberately small: the pundit colours the debate, it does not overrule maths.
CONF_NUDGE = float(os.environ.get("PUNDIT_CONF_NUDGE", "0.03"))

# Below this narrative score we escalate a challenge into a no-bet recommendation.
NO_BET_SCORE = float(os.environ.get("PUNDIT_NO_BET_SCORE", "-0.5"))


class FanPunditAgent(Specialist):
    """Qualitative narrative specialist; endorses or challenges wagers."""

    name = "fan-pundit-agent"

    async def handle(self, delegation: Delegation) -> Reply:
        proposed = _proposed_wager(delegation)
        narrative = delegation.signal.get("narrative") or delegation.raw.get("narrative")

        if proposed is None:
            # No quantitative proposal to react to — offer pure colour, no wager.
            return Reply(
                ack=(
                    "fan-pundit-agent: no quantitative proposal on the table yet; "
                    "holding narrative view until sharp signal lands"
                ),
                contribution_kind="narrative",
            )

        score = _narrative_score(narrative, proposed.selection)
        stance = "endorse" if score >= 0 else "challenge"

        # Nudge the model probability by a bounded amount in the direction of
        # the narrative. Clamp to a sane open interval so downstream Kelly maths
        # never sees 0 or 1.
        adjusted_prob = _clamp(
            proposed.model_prob + CONF_NUDGE * score, 0.01, 0.99
        )

        # A severe negative narrative turns a challenge into an explicit no-bet
        # recommendation. We surface it via status; the Authority decides.
        recommend_no_bet = score <= NO_BET_SCORE

        updated = proposed.model_copy(
            update={
                "model_prob": round(adjusted_prob, 6),
                "status": WagerStatus.NO_BET if recommend_no_bet else proposed.status,
                "thesis": (
                    f"{proposed.thesis} | Pundit {stance}: {_narrative_reason(score)}"
                ),
            }
        )

        if recommend_no_bet:
            ack = (
                f"fan-pundit-agent: CHALLENGE on {proposed.selection.value} — "
                f"{_narrative_reason(score)}; narrative risk too high, "
                f"recommending NO BET"
            )
            kind = "challenge"
        elif stance == "endorse":
            ack = (
                f"fan-pundit-agent: ENDORSE {proposed.selection.value} — "
                f"{_narrative_reason(score)}; nudging model {proposed.model_prob:.1%} "
                f"→ {adjusted_prob:.1%}"
            )
            kind = "endorse"
        else:
            ack = (
                f"fan-pundit-agent: CHALLENGE {proposed.selection.value} — "
                f"{_narrative_reason(score)}; trimming model {proposed.model_prob:.1%} "
                f"→ {adjusted_prob:.1%}"
            )
            kind = "challenge"

        return Reply(ack=ack, wager=updated, contribution_kind=kind)


def _proposed_wager(delegation: Delegation) -> Optional[Wager]:
    """Reconstruct a `Wager` from whatever the orchestrator forwarded.

    The debate coordinator forwards the sharp specialist's proposal under
    `decision.wager` (preferred) or a top-level `wager` key. Returns None when
    there is nothing quantitative to react to.
    """
    raw = None
    if isinstance(delegation.decision.get("wager"), dict):
        raw = delegation.decision["wager"]
    elif isinstance(delegation.raw.get("wager"), dict):
        raw = delegation.raw["wager"]
    if raw is None:
        return None
    try:
        return Wager.model_validate(raw)
    except Exception as exc:  # noqa: BLE001 - tolerate partial payloads
        log("fan-pundit-agent", "could not parse forwarded wager:", repr(exc))
        return None


def _narrative_score(narrative, selection: Selection) -> float:
    """Turn a narrative payload into a signed sentiment score in [-1, 1].

    Positive means the story supports backing `selection`; negative means the
    story warns against it. The payload can be:

      - a number already in [-1, 1] (pre-scored upstream), or
      - an object like `{"favours": "HOME", "strength": 0.7, "flags": [...]}`,

    Anything unrecognised yields a neutral 0.0 (endorse-by-default, no nudge).
    """
    if isinstance(narrative, (int, float)):
        return _clamp(float(narrative), -1.0, 1.0)

    if isinstance(narrative, dict):
        favours = str(narrative.get("favours", "")).upper()
        strength = _clamp(float(narrative.get("strength", 0.3) or 0.3), 0.0, 1.0)
        flags = narrative.get("flags") or []
        # Risk flags always drag the score negative regardless of side.
        risk = -0.4 * len([f for f in flags if isinstance(f, str)])
        if favours == selection.value:
            return _clamp(strength + risk, -1.0, 1.0)
        if favours in (Selection.HOME.value, Selection.DRAW.value, Selection.AWAY.value):
            # Narrative favours a different outcome → against this selection.
            return _clamp(-strength + risk, -1.0, 1.0)
        return _clamp(risk, -1.0, 1.0)

    return 0.0


_REASONS = {
    "strong_pos": "form and motivation strongly back this side",
    "pos": "narrative leans supportive",
    "neutral": "narrative is balanced",
    "neg": "context raises doubts about this side",
    "strong_neg": "classic letdown/trap dynamics against this side",
}


def _narrative_reason(score: float) -> str:
    if score >= 0.5:
        return _REASONS["strong_pos"]
    if score > 0.05:
        return _REASONS["pos"]
    if score > -0.05:
        return _REASONS["neutral"]
    if score > -0.5:
        return _REASONS["neg"]
    return _REASONS["strong_neg"]


def _clamp(value: float, lo: float, hi: float) -> float:
    return max(lo, min(hi, value))


if __name__ == "__main__":
    try:
        asyncio.run(FanPunditAgent().run())
    except KeyboardInterrupt:
        log("fan-pundit-agent", "interrupted; shutting down")
