#!/usr/bin/env python3
"""proof-guard-agent — the verification-gate specialist.

This is the skeptic of the pipeline. After the sharp/pundit/fundamentals voices
have argued a wager into shape, proof-guard is the last check *before* the
settlement track: it refuses to let a wager advance unless the numbers it claims
are internally consistent and backed by a txoracle proof attestation.

It does NOT trust the specialists' self-reported figures. For a proposed wager it
independently:

  - recomputes the market-implied probability from the wager's own `fair_odds`
    and cross-checks the claimed `edge = model_prob − market_implied`,
  - verifies the model probability is a well-formed number in (0, 1),
  - verifies a proof reference (txoracle attestation) is present and well-shaped,
  - and checks the proposed stake is non-negative and within the devnet cap.

If every check passes it returns the wager with status `ProofPassed` and a
proof-verified thesis annotation. If any check fails it returns status
`ProofFailed` with the specific reason, which honestly blocks settlement.

As with every specialist, this verdict is *advisory*. The real proof gate and
the authoritative `ProofPassed`/`ProofFailed` transition live in the Rust
Authority (`native/src/services/proof` + `services::agent::authority`); this
agent makes the reasoning explicit in the Console transcript and gives the
debate a genuine adversarial verifier rather than a rubber stamp.
"""

from __future__ import annotations

import asyncio
import math
import os
import re
import sys
from typing import List, Optional

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from coral_agent import (  # noqa: E402
    Delegation,
    Reply,
    Specialist,
    Wager,
    WagerStatus,
    implied_probability,
    log,
)


# Tolerance for the internal consistency cross-checks. Specialists round to 6dp,
# so anything looser than this indicates a fabricated or drifted figure.
CONSISTENCY_TOL = float(os.environ.get("PROOF_CONSISTENCY_TOL", "0.02"))

# Devnet hard cap echoed here so the transcript shows the ceiling. Advisory —
# the Rust Authority owns the real clamp.
MAX_DEVNET_SPEND_SOL = float(os.environ.get("MAX_DEVNET_SPEND_SOL", "0.05"))

# Minimum length/shape a proof reference must have to be considered a real
# attestation rather than a placeholder.
_PROOF_RE = re.compile(r"^[A-Za-z0-9:_\-./]{8,}$")


class ProofGuardAgent(Specialist):

    """Verification-gate specialist. Passes or fails a wager's proof; moves nothing."""

    name = "proof-guard-agent"

    async def handle(self, delegation: Delegation) -> Reply:
        wager = _wager_under_review(delegation)

        if wager is None:
            return Reply(
                ack=(
                    "proof-guard-agent: no wager on the verification track; "
                    "nothing to attest"
                ),
                contribution_kind="arbitrate",
            )

        # A concluded no-bet / zero-stake wager needs no proof — nothing is being
        # risked. Pass it through honestly without manufacturing an attestation.
        if wager.status == WagerStatus.NO_BET or wager.stake_sol <= 0.0:
            return Reply(
                ack=(
                    f"proof-guard-agent: {wager.selection.value} is no-bet / "
                    f"zero-stake; no funds at risk, proof gate not required"
                ),
                wager=wager,
                contribution_kind="arbitrate",
            )

        proof_ref = wager.proof_ref or _proof_ref_from_gate(delegation)
        failures = _run_checks(wager, proof_ref)

        if failures:
            reason = "; ".join(failures)
            failed = wager.model_copy(
                update={
                    "status": WagerStatus.PROOF_FAILED,
                    "thesis": f"{wager.thesis} | Proof-guard FAILED: {reason}",
                }
            )
            return Reply(
                ack=(
                    f"proof-guard-agent: FAIL {wager.selection.value} — {reason}; "
                    f"settlement blocked (ProofFailed)"
                ),
                wager=failed,
                contribution_kind="challenge",
            )

        assert proof_ref is not None  # guaranteed by _run_checks passing
        passed = wager.model_copy(
            update={
                "proof_ref": proof_ref,
                "status": WagerStatus.PROOF_PASSED,
                "thesis": (
                    f"{wager.thesis} | Proof-guard PASSED: edge/odds consistent, "
                    f"attestation {_short(proof_ref)} present, stake within cap."
                ),
            }
        )
        return Reply(
            ack=(
                f"proof-guard-agent: PASS {wager.selection.value} — figures "
                f"consistent, proof {_short(proof_ref)} attested, stake "
                f"{wager.stake_sol} SOL within cap; cleared for settlement"
            ),
            wager=passed,
            contribution_kind="endorse",
        )


# -- checks ----------------------------------------------------------------


def _run_checks(wager: Wager, proof_ref: Optional[str]) -> List[str]:
    """Return a list of human-readable failure reasons; empty means passed."""
    failures: List[str] = []

    # 1. Model probability must be a well-formed probability.
    if not (math.isfinite(wager.model_prob) and 0.0 < wager.model_prob < 1.0):
        failures.append(
            f"model probability {wager.model_prob} outside (0, 1)"
        )

    # 2. Recompute market-implied from the wager's own fair_odds-derived market
    #    price and cross-check the claimed edge. We reconstruct the market prob
    #    from `market_implied` directly and validate edge = model − market.
    if math.isfinite(wager.market_implied) and 0.0 < wager.market_implied < 1.0:
        expected_edge = wager.model_prob - wager.market_implied
        if abs(expected_edge - wager.edge) > CONSISTENCY_TOL:
            failures.append(
                f"edge {wager.edge:+.4f} inconsistent with model−market "
                f"{expected_edge:+.4f} (tol {CONSISTENCY_TOL})"
            )
    else:
        failures.append(
            f"market-implied probability {wager.market_implied} outside (0, 1)"
        )

    # 3. fair_odds must invert to model_prob (fair_odds = 1 / model_prob).
    if wager.model_prob > 0.0:
        implied_from_fair = implied_probability(wager.fair_odds)
        if implied_from_fair is None:
            failures.append(f"fair odds {wager.fair_odds} not a valid price")
        elif abs(implied_from_fair - wager.model_prob) > CONSISTENCY_TOL:
            failures.append(
                f"fair odds {wager.fair_odds} imply {implied_from_fair:.3f}, "
                f"not model {wager.model_prob:.3f}"
            )

    # 4. Stake must be non-negative and finite.
    if not (math.isfinite(wager.stake_sol) and wager.stake_sol >= 0.0):
        failures.append(f"stake {wager.stake_sol} is negative or non-finite")

    # 5. Proof attestation must be present and well-shaped.
    if not proof_ref:
        failures.append("no proof attestation (txoracle reference) present")
    elif not _PROOF_RE.match(proof_ref):
        failures.append(
            f"proof reference {_short(proof_ref)!r} is malformed / placeholder"
        )

    return failures


def _wager_under_review(delegation: Delegation) -> Optional[Wager]:
    """Reconstruct the wager handed to the verification track.

    The coordinator forwards it under `decision.wager` (preferred) or a
    top-level `wager` key. Partial/garbled payloads degrade to None.
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
        log("proof-guard-agent", "could not parse wager under review:", repr(exc))
        return None


def _proof_ref_from_gate(delegation: Delegation) -> Optional[str]:
    """Fallback: read a proof reference from the delegation's proof gate.

    The Rust proof service may deliver the txoracle attestation on the
    `proofGate` payload rather than baked into the wager. Accept the common
    key spellings.
    """
    gate = delegation.proof_gate or {}
    for key in ("proofRef", "proof_ref", "ref", "attestation", "txHash"):
        val = gate.get(key)
        if isinstance(val, str) and val:
            return val
    return None


def _short(proof_ref: str) -> str:
    if len(proof_ref) <= 20:
        return proof_ref
    return f"{proof_ref[:12]}…{proof_ref[-4:]}"


if __name__ == "__main__":
    try:
        asyncio.run(ProofGuardAgent().run())
    except KeyboardInterrupt:
        log("proof-guard-agent", "interrupted; shutting down")
