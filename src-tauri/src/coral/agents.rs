use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoralAgentManifest {
    pub id: &'static str,
    pub display_name: &'static str,
    pub coral_role: &'static str,
    pub service: &'static str,
    pub manifest_path: &'static str,
    pub description: &'static str,
}

pub fn built_in_agents() -> Vec<CoralAgentManifest> {
    vec![
        CoralAgentManifest {
            id: "worldcup-buyer-agent",
            display_name: "World Cup Buyer",
            coral_role: "buyer",
            service: "txline",
            manifest_path: "coral-agents/worldcup-buyer-agent/coral-agent.toml",
            description: "Turns TxLINE triggers into WANTs, collects bids, awards the best seller, and starts policy-gated settlement.",
        },
        CoralAgentManifest {
            id: "seller-worldcup-edge",
            display_name: "World Cup Edge Seller",
            coral_role: "seller",
            service: "txline.edge",
            manifest_path: "coral-agents/seller-worldcup-edge/coral-agent.toml",
            description: "Bids on odds movement WANTs and delivers a fixture-bound fair-line read.",
        },
        CoralAgentManifest {
            id: "seller-risk-policy",
            display_name: "Risk Policy Seller",
            coral_role: "seller",
            service: "risk.policy",
            manifest_path: "coral-agents/seller-risk-policy/coral-agent.toml",
            description: "Prices downside, caps exposure, and outputs no-action/observe/simulate decisions.",
        },
        CoralAgentManifest {
            id: "seller-fan-card",
            display_name: "Fan Card Seller",
            coral_role: "seller",
            service: "fan.card",
            manifest_path: "coral-agents/seller-fan-card/coral-agent.toml",
            description: "Converts match events into shareable fan-facing explanations.",
        },
        CoralAgentManifest {
            id: "verifier-agent",
            display_name: "Verifier",
            coral_role: "verifier",
            service: "delivery.verify",
            manifest_path: "coral-agents/verifier-agent/coral-agent.toml",
            description: "Checks delivery hash, fixture binding, proof structure, and policy gates before release.",
        },
        CoralAgentManifest {
            id: "settlement-arbiter-agent",
            display_name: "Settlement Arbiter",
            coral_role: "settlement",
            service: "settlement.release",
            manifest_path: "coral-agents/settlement-arbiter-agent/coral-agent.toml",
            description: "Bridges a verified run to the CoralOS settlement sidecar and devnet escrow observation.",
        },
    ]
}
