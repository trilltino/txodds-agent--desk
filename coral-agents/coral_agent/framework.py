"""CoralOS specialist runtime: connect, receive DELEGATE, reply.

This is the reusable spine every specialist agent builds on. It replaces the
old idle `agent.py` loop (connect → list tools → `await Event().wait()`) with a
real message loop that:

  1. connects to CoralOS over streamable HTTP MCP (same transport as before),
  2. announces itself so the orchestrator can address it,
  3. polls the Coral thread for `TOOL_CALL` messages whose text starts with
     `DELEGATE` and whose recipient list contains this agent,
  4. parses the payload into a `Delegation`,
  5. calls the subclass's async `handle(delegation)` — which returns a
     `Wager` proposal plus a human-readable ack line,
  6. publishes the ack as a `TOOL_RESULT` back to `match-intelligence-agent`
     and `user-proxy`, attaching the proposed `Wager` as structured payload.

The Rust Authority remains the only component that sizes stakes, clears the
proof gate, or advances a wager past `Debated`. A `Wager` returned here is a
*proposal*: the Authority re-derives the Kelly stake, clamps it to the devnet
cap, and can veto it entirely. This class never touches a wallet or a chain.

The MCP tool surface CoralOS exposes for threads varies by deployment, so the
send/receive helpers below try the common tool names and degrade gracefully:
if no thread tool is available the agent still connects and logs, preserving
the old idle behaviour rather than crashing a Console session.
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Tuple

from .wager import Wager


def log(agent: str, *parts: Any) -> None:
    """Structured stderr log line, matching the old stub's `[agent] ...` format."""
    print(f"[{agent}]", *parts, file=sys.stderr, flush=True)


@dataclass
class Delegation:
    """A parsed `DELEGATE` hand-off from the orchestrating agent.

    Built from the `TOOL_CALL` message payload emitted by `runtime.rs`
    (`specialist_for_track` branch). Fields are optional-tolerant because the
    orchestrator's payload shape may grow; unknown keys are preserved in `raw`.
    """

    specialist: str
    track: str
    fixture_id: Optional[int]
    message_id: str
    round: int
    signal: Dict[str, Any] = field(default_factory=dict)
    decision: Dict[str, Any] = field(default_factory=dict)
    proof_gate: Dict[str, Any] = field(default_factory=dict)
    raw: Dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_message(cls, msg: Dict[str, Any]) -> "Delegation":
        payload = msg.get("payload") or {}
        return cls(
            specialist=payload.get("specialist", ""),
            track=str(payload.get("track", "")),
            fixture_id=_maybe_int(
                payload.get("fixtureId")
                or (payload.get("signal") or {}).get("fixtureId")
            ),
            message_id=msg.get("id", ""),
            round=int(msg.get("round", 0) or 0),
            signal=payload.get("signal") or {},
            decision=payload.get("decision") or {},
            proof_gate=payload.get("proofGate") or {},
            raw=payload,
        )


@dataclass
class Reply:
    """A specialist's answer to a delegation.

    `ack` is the transcript line shown in Console (replacing the Rust
    `specialist_ack`). `wager` is the optional structured proposal the debate
    coordinator / Authority will consume. `contribution_kind` labels this move
    in the debate (e.g. `endorse`, `challenge`, `narrative`).
    """

    ack: str
    wager: Optional[Wager] = None
    contribution_kind: str = "analysis"
    extra: Dict[str, Any] = field(default_factory=dict)


class Specialist(ABC):
    """Base class for a track specialist agent.

    Subclasses set `name` (must equal the CoralOS participant id, e.g.
    `settlement-agent`) and implement `handle`. Everything else — connection,
    the receive loop, deduping already-handled delegations, and publishing the
    reply — is provided here.
    """

    #: CoralOS participant id. MUST match `protocol.rs` and `coral-agent.toml`.
    name: str = "specialist"

    #: Recipients for the reply. The Rust runtime addressed acks to both the
    #: orchestrator and the user proxy; we preserve that so Console renders it.
    reply_to: Tuple[str, ...] = ("match-intelligence-agent", "user-proxy")

    #: Poll interval (seconds) when the deployment has no push/streaming tool.
    poll_interval: float = 1.0

    def __init__(self) -> None:
        self._seen: set[str] = set()

    @abstractmethod
    async def handle(self, delegation: Delegation) -> Reply:
        """Produce a reply for one delegation. Implemented by each specialist."""
        raise NotImplementedError


    # -- runtime ----------------------------------------------------------

    async def run(self) -> None:
        """Connect to CoralOS and service delegations until cancelled."""
        from mcp import ClientSession
        from mcp.client.streamable_http import streamablehttp_client

        url = os.environ.get("CORAL_CONNECTION_URL")
        if not url:
            log(self.name, "CORAL_CONNECTION_URL not set; CoralOS must launch this participant")
            sys.exit(1)

        log(self.name, "connecting to CoralOS at", url)
        async with streamablehttp_client(url) as (read, write, _):
            async with ClientSession(read, write) as session:
                await session.initialize()
                tools = await session.list_tools()
                tool_names = [t.name for t in tools.tools]
                log(self.name, "connected; tools:", tool_names)
                await self._serve(session, tool_names)

    async def _serve(self, session: Any, tool_names: List[str]) -> None:
        """Main receive loop. Falls back to idle if no thread tool exists."""
        recv_tool = _first_present(
            tool_names, ["coral_wait_for_mentions", "wait_for_mentions", "read_thread", "list_messages"]
        )
        send_tool = _first_present(
            tool_names, ["coral_send_message", "send_message", "post_message"]
        )
        if recv_tool is None:
            # Preserve the historical idle behaviour: the Rust puppet API is
            # still publishing this agent's messages, so we must not crash.
            log(self.name, "no thread-read tool available; idling (Rust puppet drives transcript)")
            await asyncio.Event().wait()
            return

        log(self.name, "serving delegations via", recv_tool, "->", send_tool)
        while True:
            try:
                messages = await self._read_messages(session, recv_tool)
            except Exception as exc:  # noqa: BLE001 - keep the loop alive
                log(self.name, "read error (retrying):", repr(exc))
                await asyncio.sleep(self.poll_interval)
                continue

            for msg in messages:
                if not self._is_delegation_for_me(msg):
                    continue
                mid = msg.get("id", "")
                if mid in self._seen:
                    continue
                self._seen.add(mid)
                await self._dispatch(session, send_tool, msg)

            await asyncio.sleep(self.poll_interval)

    async def _dispatch(self, session: Any, send_tool: Optional[str], msg: Dict[str, Any]) -> None:
        delegation = Delegation.from_message(msg)
        log(self.name, "delegation received for fixture", delegation.fixture_id)
        try:
            reply = await self.handle(delegation)
        except Exception as exc:  # noqa: BLE001 - report, never die
            log(self.name, "handle() failed:", repr(exc))
            reply = Reply(ack=f"{self.name}: internal error handling delegation ({exc})")

        payload: Dict[str, Any] = {
            "specialist": self.name,
            "status": "acknowledged",
            "track": delegation.track,
            "contributionKind": reply.contribution_kind,
            "inReplyTo": delegation.message_id,
        }
        if reply.wager is not None:
            payload["wager"] = reply.wager.model_dump(by_alias=True)
        payload.update(reply.extra)

        if send_tool is None:
            log(self.name, "no send tool; would reply:", reply.ack)
            return
        await self._send_message(
            session, send_tool, self.reply_to, "TOOL_RESULT", reply.ack, payload
        )
        log(self.name, "reply published:", reply.ack)

    # -- MCP tool adapters ------------------------------------------------

    async def _read_messages(self, session: Any, tool: str) -> List[Dict[str, Any]]:
        result = await session.call_tool(tool, {"agentId": self.name})
        return _extract_messages(result)

    async def _send_message(
        self,
        session: Any,
        tool: str,
        to: Tuple[str, ...],
        verb: str,
        text: str,
        payload: Dict[str, Any],
    ) -> None:
        await session.call_tool(
            tool,
            {
                "from": self.name,
                "to": list(to),
                "verb": verb,
                "text": text,
                "payload": payload,
            },
        )

    # -- filters ----------------------------------------------------------

    def _is_delegation_for_me(self, msg: Dict[str, Any]) -> bool:
        verb = str(msg.get("verb", "")).upper()
        text = str(msg.get("text", ""))
        to = msg.get("to") or []
        return (
            verb in ("TOOL_CALL", "TOOLCALL")
            and text.startswith("DELEGATE")
            and self.name in to
        )


# -- module-level helpers --------------------------------------------------


def _maybe_int(value: Any) -> Optional[int]:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _first_present(names: List[str], candidates: List[str]) -> Optional[str]:
    for candidate in candidates:
        if candidate in names:
            return candidate
    return None


def _extract_messages(result: Any) -> List[Dict[str, Any]]:
    """Coax a list of message dicts out of an MCP `call_tool` result.

    MCP returns content blocks; thread tools typically pack JSON into a text
    block. We tolerate either a bare list or an object with a `messages` key.
    """
    content = getattr(result, "content", None)
    if not content:
        return []
    out: List[Dict[str, Any]] = []
    for block in content:
        raw = getattr(block, "text", None)
        if not raw:
            continue
        try:
            parsed = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, list):
            out.extend(m for m in parsed if isinstance(m, dict))
        elif isinstance(parsed, dict):
            msgs = parsed.get("messages")
            if isinstance(msgs, list):
                out.extend(m for m in msgs if isinstance(m, dict))
            else:
                out.append(parsed)
    return out
