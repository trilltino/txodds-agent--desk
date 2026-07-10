"""Pydantic mirror of the Rust `txodds_types::Wager` domain model.

This module is a *contract*, not a reimplementation. Every field name, enum
spelling, and serialization alias here must match `crates/txodds-types/src/
wager.rs` byte-for-byte, because a `Wager` proposed by a Python specialist is
deserialized by the Rust Authority (`services::agent::authority`) before it is
ever acted on. If the two drift, proposals fail to parse and the Authority
rejects them — which is safe, but breaks the debate.

Serialization rules copied from the Rust `#[serde(...)]` attributes:

- `Selection`     → UPPERCASE   ("HOME" | "DRAW" | "AWAY")
- `WagerStatus`   → snake_case  ("proposed" | "proof_passed" | ...)
- struct fields   → camelCase   (wager_id → "wagerId", model_prob → "modelProb")

The Kelly and implied-probability helpers are duplicated here so a specialist
can *reason* about sizing, but the authoritative stake is always the value the
Rust Authority computes and clamps. Never treat the Python stake as final.
"""

from __future__ import annotations

from enum import Enum
from typing import List, Optional

from pydantic import BaseModel, ConfigDict, Field
from pydantic.alias_generators import to_camel


class Selection(str, Enum):
    """The outcome a wager backs. Mirrors the Rust 1X2 `Selection` enum."""

    HOME = "HOME"
    DRAW = "DRAW"
    AWAY = "AWAY"


class WagerStatus(str, Enum):
    """Lifecycle state, mirroring the Rust `WagerStatus` (snake_case wire form).

    Proposed → Debated → ProofPassed → Signed → Settled, with the honest
    terminal states NoBet / ProofFailed / Refunded.
    """

    PROPOSED = "proposed"
    DEBATED = "debated"
    NO_BET = "no_bet"
    PROOF_PASSED = "proof_passed"
    PROOF_FAILED = "proof_failed"
    SIGNED = "signed"
    SETTLED = "settled"
    REFUNDED = "refunded"


class _CamelModel(BaseModel):
    """Base that emits/accepts camelCase JSON while keeping snake_case in Python.

    `populate_by_name=True` lets tests and internal code construct instances with
    the Python field names, while `by_alias=True` on dump produces the camelCase
    the Rust side expects.
    """

    model_config = ConfigDict(
        alias_generator=to_camel,
        populate_by_name=True,
        use_enum_values=True,
    )


class DebateContribution(_CamelModel):
    """One specialist's contribution to a debate round (mirrors Rust)."""

    agent_id: str
    round: int
    # analysis | signal | narrative | challenge | endorse | arbitrate
    kind: str
    summary: str
    prob: Optional[float] = None
    confidence: Optional[float] = None
    targets: List[str] = Field(default_factory=list)


class DebateSummary(_CamelModel):
    """Full transcript of the adversarial debate that produced a wager."""

    rounds: int
    converged: bool
    contributions: List[DebateContribution] = Field(default_factory=list)


class Wager(_CamelModel):
    """A proof-verified, Kelly-sized wager — the object settlement acts on.

    A specialist populates `model_prob`, `market_implied`, `edge`, `fair_odds`,
    `thesis`, and a *proposed* `stake_sol`. The Rust Authority recomputes the
    stake from `kelly_fraction`, clamps it to `max_devnet_spend_sol`, and owns
    the status transitions past `Debated`.
    """

    wager_id: str
    fixture_id: int
    selection: Selection
    model_prob: float
    market_implied: float
    edge: float
    fair_odds: float
    stake_sol: float
    thesis: str
    proof_ref: Optional[str] = None
    status: WagerStatus
    debate: Optional[DebateSummary] = None
    created_at: str

    def has_value(self, min_edge: float) -> bool:
        """True when edge clears `min_edge` and the probability is well-formed.

        Pure helper mirroring `Wager::has_value` in Rust — no policy here.
        """
        return self.edge > min_edge and 0.0 < self.model_prob < 1.0


def kelly_fraction(model_prob: float, fair_odds: float) -> float:
    """Kelly stake fraction for a single binary outcome.

    `f* = (b·p − q) / b`, `b = fair_odds − 1`, `q = 1 − p`. Clamped to
    [0.0, 1.0]; non-finite or degenerate inputs yield 0.0 (no bet). Identical
    to the Rust `kelly_fraction` so Python reasoning matches Authority sizing.
    """
    import math

    if not (math.isfinite(model_prob) and math.isfinite(fair_odds)):
        return 0.0
    if model_prob <= 0.0 or model_prob >= 1.0 or fair_odds <= 1.0:
        return 0.0
    b = fair_odds - 1.0
    q = 1.0 - model_prob
    f = (b * model_prob - q) / b
    return max(0.0, min(1.0, f))


def implied_probability(decimal_odds: float) -> Optional[float]:
    """Decimal odds → implied probability (`1 / odds`), ignoring the overround.

    Returns None for non-finite or `<= 1.0` odds. Mirrors the Rust helper.
    """
    import math

    if math.isfinite(decimal_odds) and decimal_odds > 1.0:
        return 1.0 / decimal_odds
    return None
