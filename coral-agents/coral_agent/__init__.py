"""Shared framework for TxODDS CoralOS specialist agents.

Every specialist (`settlement-agent`, `sharp-movement-detector`,
`fan-pundit-agent`, ...) imports this package instead of copy-pasting the
CoralOS connection boilerplate. It provides:

- `Wager` / `WagerStatus` / `Selection`: Pydantic models that mirror the Rust
  `txodds_types::Wager` exactly, so proposals serialise byte-compatibly across
  the Python↔Rust boundary.
- `Delegation`: the parsed payload of a `DELEGATE` tool-call handed off from the
  orchestrating `match-intelligence-agent`.
- `Specialist`: an async base class that connects to CoralOS, listens for
  delegations addressed to it, runs the subclass's `handle()` coroutine, and
  publishes the reply back onto the Coral bus.

The safety spine still lives in Rust (`services::agent::authority`). A specialist
may *propose* a `Wager` with a model probability and a thesis; it may never size
the stake, bypass the proof gate, or exceed the devnet spend cap. Those are
recomputed and clamped by the Rust Authority regardless of what a specialist
sends.
"""

from .wager import (
    Selection,
    Wager,
    WagerStatus,
    DebateContribution,
    DebateSummary,
    kelly_fraction,
    implied_probability,
)
from .framework import Delegation, Reply, Specialist, log

__all__ = [
    "Selection",
    "Wager",
    "WagerStatus",
    "DebateContribution",
    "DebateSummary",
    "kelly_fraction",
    "implied_probability",
    "Delegation",
    "Reply",
    "Specialist",
    "log",
]


