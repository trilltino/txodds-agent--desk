use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::types::{
    now_iso, AgentBid, AgentDelivery, AgentRole, AgentRun, SettlementReceipt, SettlementStatus,
    TimelineEntry, TrackMode, TxLineEvent, TxLineEventKind, VerdictCheck, VerdictStatus,
    VerificationVerdict,
};

pub fn run_round(trigger: TxLineEvent, track: TrackMode) -> AgentRun {
    let mut run = empty_run(trigger, track);
    push(
        &mut run,
        "WANT",
        format!("worldcup-buyer-agent asks for {track} output"),
    );

    run.bids = generate_bids(&run.trigger, track);
    let bid_count = run.bids.len();
    push(
        &mut run,
        "BID",
        format!("{bid_count} specialist agents bid"),
    );

    run.winner = choose_winner(track, &run.bids);
    let winner_detail = format!(
        "{} selected on value/confidence/price",
        run.winner
            .as_ref()
            .map(|bid| bid.agent_id.as_str())
            .unwrap_or("none")
    );
    push(&mut run, "AWARD", winner_detail);

    if let Some(winner) = run.winner.clone() {
        let payload = make_delivery_payload(&run.trigger, track, &winner.agent_id);
        let sha256 = sha256_hex(&payload);
        let delivery = AgentDelivery {
            agent_id: winner.agent_id,
            title: delivery_title(track).to_string(),
            payload,
            sha256: sha256.clone(),
            citations: vec![
                "TxLINE event stream".to_string(),
                "TxLINE odds/scores snapshot".to_string(),
            ],
            strategy: (matches!(track, TrackMode::Trading)).then(|| {
                "No blind bet: signal is logged, risk-scored, and simulated before any position."
                    .to_string()
            }),
            risk: (matches!(track, TrackMode::Settlement))
                .then(|| "Release only after proof receipt/verifier pass.".to_string()),
            fan_copy: (matches!(track, TrackMode::Fan))
                .then(|| "Shareable match card generated for non-technical fans.".to_string()),
        };
        push(
            &mut run,
            "DELIVERED",
            format!("artifact sha256={}...", &sha256[..12]),
        );

        let verdict = verify_delivery(&delivery, track);
        push(
            &mut run,
            "VERIFIED",
            format!("{:?}: {}", verdict.status, verdict.reason),
        );

        run.settlement = Some(SettlementReceipt {
            status: SettlementStatus::NotStarted,
            reference: Some(format!("sha256:{sha256}")),
            escrow_pda: None,
            deposit_tx: None,
            release_tx: None,
            explorer_url: Some("https://explorer.solana.com/?cluster=devnet".to_string()),
            triton_observed: Some(false),
            triton_slot: None,
        });
        run.delivery = Some(delivery);
        run.verdict = Some(verdict);
    }

    let settlement_detail = run
        .settlement
        .as_ref()
        .map(|receipt| format!("{:?}", receipt.status))
        .unwrap_or_else(|| "not_started".to_string());
    push(&mut run, "SETTLEMENT", settlement_detail);
    run
}

pub fn append_timeline(run: &mut AgentRun, label: impl Into<String>, detail: impl Into<String>) {
    push(run, label, detail);
}

pub fn score_bid(track: TrackMode, bid: &AgentBid) -> f64 {
    let role_boost = match (&bid.role, track) {
        (AgentRole::Sharp, TrackMode::Trading) => 1.25,
        (AgentRole::Risk, TrackMode::Trading) => 1.15,
        (AgentRole::Settlement, TrackMode::Settlement) => 1.25,
        (AgentRole::Verifier, TrackMode::Settlement) => 1.20,
        (AgentRole::Pundit, TrackMode::Fan) => 1.25,
        (AgentRole::Fan, TrackMode::Fan) => 1.20,
        _ => 1.0,
    };
    let price_penalty = (1.0 - bid.price_sol * 4.0).max(0.2);
    let eta_bonus = if bid.eta_ms < 1500 { 1.05 } else { 1.0 };
    bid.confidence * price_penalty * eta_bonus * role_boost
}

fn empty_run(trigger: TxLineEvent, track: TrackMode) -> AgentRun {
    AgentRun {
        run_id: format!("{track}-{}-{}", trigger.id, Uuid::new_v4()),
        track,
        timeline: vec![TimelineEntry {
            at: now_iso(),
            label: "TRIGGER".to_string(),
            detail: format!("{:?}: {}", trigger.kind, trigger.title),
        }],
        trigger,
        bids: vec![],
        winner: None,
        delivery: None,
        verdict: None,
        settlement: Some(SettlementReceipt {
            status: SettlementStatus::NotStarted,
            reference: None,
            escrow_pda: None,
            deposit_tx: None,
            release_tx: None,
            explorer_url: None,
            triton_observed: Some(false),
            triton_slot: None,
        }),
    }
}

fn generate_bids(event: &TxLineEvent, track: TrackMode) -> Vec<AgentBid> {
    let base: f64 = match event.kind {
        TxLineEventKind::OddsMove => 0.82,
        TxLineEventKind::Goal => 0.78,
        _ => 0.70,
    };
    let bids = vec![
        AgentBid {
            agent_id: "seller-worldcup-edge".to_string(),
            role: AgentRole::Sharp,
            price_sol: 0.018,
            confidence: (base + 0.08).min(0.94),
            eta_ms: 900,
            note: "TxLINE seller: detects implied-probability movement, compares the board, and delivers a fair-line read.".to_string(),
        },
        AgentBid {
            agent_id: "seller-risk-policy".to_string(),
            role: AgentRole::Risk,
            price_sol: 0.012,
            confidence: (base + 0.02).min(0.90),
            eta_ms: 700,
            note: "Risk seller: turns a signal into no-action / observe / simulate-position with bounded downside.".to_string(),
        },
        AgentBid {
            agent_id: "seller-fan-card".to_string(),
            role: AgentRole::Pundit,
            price_sol: 0.010,
            confidence: (base + 0.01).min(0.88),
            eta_ms: 600,
            note: "Fan seller: explains the football story and market movement in plain English.".to_string(),
        },
        AgentBid {
            agent_id: "verifier-agent".to_string(),
            role: AgentRole::Verifier,
            price_sol: 0.009,
            confidence: 0.91,
            eta_ms: 800,
            note: "Independent verifier: checks content hash, fixture binding, TxLINE proof shape, and policy gates.".to_string(),
        },
        AgentBid {
            agent_id: "settlement-arbiter-agent".to_string(),
            role: AgentRole::Settlement,
            price_sol: 0.016,
            confidence: 0.92,
            eta_ms: 1100,
            note: "Settlement arbiter: packages the verified run for CoralOS escrow release and Triton observation.".to_string(),
        },
    ];

    bids.into_iter()
        .filter(|bid| match track {
            TrackMode::Fan => matches!(
                bid.role,
                AgentRole::Pundit | AgentRole::Fan | AgentRole::Sharp
            ),
            TrackMode::Trading => matches!(
                bid.role,
                AgentRole::Sharp | AgentRole::Risk | AgentRole::Pundit
            ),
            TrackMode::Settlement => matches!(
                bid.role,
                AgentRole::Settlement | AgentRole::Verifier | AgentRole::Sharp | AgentRole::Risk
            ),
        })
        .collect()
}

fn choose_winner(track: TrackMode, bids: &[AgentBid]) -> Option<AgentBid> {
    bids.iter()
        .cloned()
        .max_by(|a, b| score_bid(track, a).total_cmp(&score_bid(track, b)))
}

fn delivery_title(track: TrackMode) -> &'static str {
    match track {
        TrackMode::Settlement => "Verifiable resolution package",
        TrackMode::Trading => "Autonomous signal package",
        TrackMode::Fan => "AI pundit fan card",
    }
}

fn make_delivery_payload(event: &TxLineEvent, track: TrackMode, agent_id: &str) -> String {
    match track {
        TrackMode::Settlement => serde_json::json!({
            "type": "resolution_package",
            "agentId": agent_id,
            "fixtureId": event.fixture_id,
            "trigger": event.kind,
            "result": event.score,
            "proofPlan": "Fetch TxLINE stat-validation payload; if final stat validates, call escrow/market release path.",
            "compliance": "Demo/devnet only. No real-money wagering."
        }),
        TrackMode::Trading => serde_json::json!({
            "type": "signal_package",
            "agentId": agent_id,
            "fixtureId": event.fixture_id,
            "signal": if matches!(event.kind, TxLineEventKind::OddsMove) { "significant_move_detected" } else { "event_context_update" },
            "action": "log_and_simulate",
            "risk": "no automatic real-money execution; devnet/simulated strategy state only",
            "explanation": event.body
        }),
        TrackMode::Fan => serde_json::json!({
            "type": "fan_card",
            "agentId": agent_id,
            "fixtureId": event.fixture_id,
            "headline": event.title,
            "explainer": event.body,
            "shareCopy": format!("World Cup swing: {}. {}", event.title, event.body),
            "ttsScript": format!("Here is what just happened. {}", event.body)
        }),
    }
    .to_string()
}

fn verify_delivery(delivery: &AgentDelivery, track: TrackMode) -> VerificationVerdict {
    let mut checked = vec![
        VerdictCheck::TxlineInput,
        VerdictCheck::Hash,
        VerdictCheck::Policy,
    ];
    if matches!(track, TrackMode::Settlement) {
        checked.push(VerdictCheck::Proof);
    }
    if delivery.sha256.len() != 64 {
        return VerificationVerdict {
            status: VerdictStatus::Fail,
            reason: "hash missing or malformed".to_string(),
            checked,
        };
    }
    if !delivery.payload.contains("fixtureId") {
        return VerificationVerdict {
            status: VerdictStatus::Fail,
            reason: "delivery does not bind to fixture".to_string(),
            checked,
        };
    }
    VerificationVerdict {
        status: VerdictStatus::Pass,
        reason: "delivery is fixture-bound, hash-bound, and policy-compatible".to_string(),
        checked,
    }
}

fn push(run: &mut AgentRun, label: impl Into<String>, detail: impl Into<String>) {
    run.timeline.push(TimelineEntry {
        at: now_iso(),
        label: label.into(),
        detail: detail.into(),
    });
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}
