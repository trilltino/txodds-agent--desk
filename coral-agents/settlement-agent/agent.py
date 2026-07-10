#!/usr/bin/env python3
"""settlement-agent — the proof-gated settlement specialist.

This is the final voice in the pipeline, and deliberately the most restrained.
Where the sharp and pundit agents *argue* about value, this agent's only job is
to look at a wager that has survived the debate and decide whether it is in a
state where an on-chain escrow release could even be *proposed* to the user.

It NEVER moves funds. `allow_settlement_release = false` in coral-agent.toml,
and the Rust runtime + the user's Phantom signature are the only things that
can advance a wager to `Signed`/`Settled`. This specialist produces an
acknowledgement:

  - if the wager has no proof attestation (`proof_ref` missing) → it refuses to
    endorse settlement and surfaces `ProofFailed`-shaped caution (the Authority
    still owns the real verdict),
  - if the proof is present but the debate concluded `NoBet` / zero stake →
    nothing to settle, it acknowledges the null result,
  - if the wager is proof-passed with a positive stake → it acknowledges the
    hand-off, echoing the stake/selection the Authority will ask the user to
    sign. It leaves `status` at `ProofPassed`; it does not fabricate `Signed`.

The contribution kind is `arbitrate` — settlement is the closing arbitration of
the debate, not a new argument. As with every specialist, the returned `Wager`
is a proposal: the Rust Authority re-derives and clamps, and the user signs.
"""

from __future__ import annotations

import asyncio
import os
import sys
from typing import Optional

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from coral_agent import (  # noqa: E402
    Delegation,
    Reply,
    Specialist,
    Wager,
    WagerStatus,
    log,
)

# Devnet hard cap echoed in the acknowledgement so the transcript shows the
# ceiling the Authority will clamp to. Advisory only — Rust owns the real clamp.
MAX_DEVNET_SPEND_SOL = float(os.environ.get("MAX_DEVNET_SPEND_SOL", "0.05"))


class SettlementAgent(Specialist):
    """Proof-gated settlement specialist. Acknowledges; never releases funds."""

    name = "settlement-agent"

    async def handle(self, delegation: Delegation) -> Reply:
        wager = _wager_under_settlement(delegation)

        if wager is None:
            return Reply(
                ack=(
                    "settlement-agent: no wager on the settlement track; "
                    "nothing to acknowledge"
                ),
                contribution_kind="arbitrate",
            )

        # 1. Proof gate. We will not endorse settlement for an unproven wager.
        #    We do NOT set ProofFailed ourselves (proof-guard/Authority own that
        #    verdict); we withhold endorsement and flag the missing attestation.
        proof_ref = wager.proof_ref or _proof_ref_from_gate(delegation)
        if not proof_ref:
            return Reply(
                ack=(
                    f"settlement-agent: WITHHOLD on {wager.selection.value} — "
                    f"no proof attestation present; settlement blocked until "
                    f"proof-guard clears the gate"
                ),
                wager=wager,
                contribution_kind="arbitrate",
            )

        # 2. Null result. A proven-but-NoBet / zero-stake wager has nothing to
        #    settle. Acknowledge honestly rather than manufacture an obligation.
        if wager.status == WagerStatus.NO_BET or wager.stake_sol <= 0.0:
            return Reply(
                ack=(
                    f"settlement-agent: {wager.selection.value} concluded "
                    f"no-bet / zero stake; no escrow to open, nothing to settle"
                ),
                wager=wager,
                contribution_kind="arbitrate",
            )

        # 3. Proof-passed, positive stake → acknowledge the hand-off. We echo the
        #    stake the Authority will present for signature, and note the cap.
        #    Status stays ProofPassed: only the Rust runtime + Phantom signature
        #    advance it to Signed, and only result verification advances Settled.
        clamped = min(wager.stake_sol, MAX_DEVNET_SPEND_SOL)
        acknowledged = wager.model_copy(
            update={
                "proof_ref": proof_ref,
                "status": WagerStatus.PROOF_PASSED,
                "thesis": (
                    f"{wager.thesis} | Settlement: proof {_short(proof_ref)} "
                    f"verified, ready for user signature on {clamped} SOL "
                    f"(cap {MAX_DEVNET_SPEND_SOL} SOL)."
                ),
            }
        )

        note = "" if clamped == wager.stake_sol else " [above cap — Authority will clamp]"
        return Reply(
            ack=(
                f"settlement-agent: READY {wager.selection.value} @ "
                f"stake {wager.stake_sol} SOL{note} — proof {_short(proof_ref)} "
                f"attested; awaiting user signature (no funds moved by this agent)"
            ),
            wager=acknowledged,
            contribution_kind="arbitrate",
        )


def _wager_under_settlement(delegation: Delegation) -> Optional[Wager]:
    """Reconstruct the wager the settlement track was handed.

    The coordinator forwards the surviving wager under `decision.wager`
    (preferred) or a top-level `wager` key. Returns None if there is nothing to
    settle. Partial/garbled payloads degrade to None rather than crashing.
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
        log("settlement-agent", "could not parse settlement wager:", repr(exc))
        return None


def _proof_ref_from_gate(delegation: Delegation) -> Optional[str]:
    """Fallback: read a proof reference from the delegation's proof gate.

    proof-guard's verdict may arrive on the `proofGate` payload rather than
    baked into the forwarded wager. Accept `proofRef` / `proof_ref` / `ref`.
    """
    gate = delegation.proof_gate or {}
    for key in ("proofRef", "proof_ref", "ref"):
        val = gate.get(key)
        if isinstance(val, str) and val:
            return val
    return None


def _short(proof_ref: str) -> str:
    """Truncate a long attestation reference for readable transcript lines."""
    if len(proof_ref) <= 20:
        return proof_ref
    return f"{proof_ref[:12]}…{proof_ref[-4:]}"


if __name__ == "__main__":
    try:
        asyncio.run(SettlementAgent().run())
    except KeyboardInterrupt:
        log("settlement-agent", "interrupted; shutting down")
