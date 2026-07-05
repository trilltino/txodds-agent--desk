use serde::Serialize;
use uuid::Uuid;

use crate::types::Cluster;

const KEYRING_SERVICE: &str = "World Cup Agent Desk";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub txline_api_origin: String,
    pub txline_network: String,
    pub txline_guest_jwt: Option<String>,
    pub txline_api_token: Option<String>,
    pub solana_cluster: String,
    pub triton_devnet_rpc: Option<String>,
    pub triton_devnet_token: Option<String>,
    pub triton_mainnet_rpc: Option<String>,
    pub triton_mainnet_token: Option<String>,
    pub triton_grpc_endpoint: Option<String>,
    pub triton_x_token: Option<String>,
    pub watch_escrow_program_id: Option<String>,
    pub watch_market_program_id: Option<String>,
    pub watch_escrow_account: Option<String>,
    pub coralos_root: Option<String>,
    pub coralos_bridge_url: Option<String>,
    pub coralos_proxy_url: String,
    pub coralos_server_url: String,
    pub coralos_token: String,
    pub coralos_sidecar_path: Option<String>,
    pub coralos_settlement_enabled: bool,
    pub odds_move_trigger_pct: f64,
    pub max_devnet_spend_sol: f64,
    pub axum_enabled: bool,
    pub axum_token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicConfig {
    pub txline_api_origin: String,
    pub txline_network: String,
    pub solana_cluster: String,
    pub odds_move_trigger_pct: f64,
    pub max_devnet_spend_sol: f64,
    pub txline_configured: bool,
    pub triton_configured: bool,
    pub triton_devnet_configured: bool,
    pub triton_mainnet_configured: bool,
    pub yellowstone_configured: bool,
    pub coralos_configured: bool,
    pub axum_enabled: bool,
}

impl AppConfig {
    pub fn load() -> Self {
        let _ = dotenvy::dotenv();

        Self {
            txline_api_origin: env_or_default("TXLINE_API_ORIGIN", "https://txline-dev.txodds.com"),
            txline_network: env_or_default("TXLINE_NETWORK", "devnet"),
            txline_guest_jwt: secret("TXLINE_GUEST_JWT", "txline_guest_jwt"),
            txline_api_token: secret("TXLINE_API_TOKEN", "txline_api_token"),
            solana_cluster: env_or_default("SOLANA_CLUSTER", "devnet"),
            triton_devnet_rpc: optional_env("TRITON_DEVNET_RPC"),
            triton_devnet_token: secret("TRITON_DEVNET_TOKEN", "triton_devnet_token"),
            triton_mainnet_rpc: optional_env("TRITON_MAINNET_RPC"),
            triton_mainnet_token: secret("TRITON_MAINNET_TOKEN", "triton_mainnet_token"),
            triton_grpc_endpoint: optional_env("TRITON_GRPC_ENDPOINT"),
            triton_x_token: secret("TRITON_X_TOKEN", "triton_x_token"),
            watch_escrow_program_id: optional_env("WATCH_ESCROW_PROGRAM_ID"),
            watch_market_program_id: optional_env("WATCH_MARKET_PROGRAM_ID"),
            watch_escrow_account: optional_env("WATCH_ESCROW_ACCOUNT"),
            coralos_root: optional_env("CORALOS_ROOT"),
            coralos_bridge_url: optional_env("CORALOS_BRIDGE_URL"),
            coralos_proxy_url: env_or_default("CORALOS_TXODDS_PROXY", "http://localhost:8801"),
            coralos_server_url: env_or_default("CORAL_SERVER_URL", "http://localhost:5555"),
            coralos_token: env_or_default("CORAL_TOKEN", "dev"),
            coralos_sidecar_path: optional_env("CORALOS_SIDECAR_PATH"),
            coralos_settlement_enabled: bool_env("CORALOS_SETTLEMENT_ENABLED", true),
            odds_move_trigger_pct: number_env("ODDS_MOVE_TRIGGER_PCT", 5.0),
            max_devnet_spend_sol: number_env("MAX_DEVNET_SPEND_SOL", 0.05),
            axum_enabled: bool_env("DESK_AXUM_ENABLED", false),
            axum_token: Uuid::new_v4().to_string(),
        }
    }

    pub fn public(&self) -> PublicConfig {
        PublicConfig {
            txline_api_origin: self.txline_api_origin.clone(),
            txline_network: self.txline_network.clone(),
            solana_cluster: self.solana_cluster.clone(),
            odds_move_trigger_pct: self.odds_move_trigger_pct,
            max_devnet_spend_sol: self.max_devnet_spend_sol,
            txline_configured: self.txline_guest_jwt.is_some() && self.txline_api_token.is_some(),
            triton_configured: self.triton_pair_configured(Cluster::Devnet)
                || self.triton_pair_configured(Cluster::Mainnet),
            triton_devnet_configured: self.triton_pair_configured(Cluster::Devnet),
            triton_mainnet_configured: self.triton_pair_configured(Cluster::Mainnet),
            yellowstone_configured: self.triton_grpc_endpoint.is_some()
                && self.triton_x_token.is_some(),
            coralos_configured: self.coralos_settlement_enabled
                && (self.coralos_bridge_url.is_some() || self.coralos_root.is_some()),
            axum_enabled: self.axum_enabled,
        }
    }

    pub fn triton_pair_configured(&self, cluster: Cluster) -> bool {
        self.triton_endpoint(cluster).is_some() && self.triton_token(cluster).is_some()
    }

    pub fn triton_endpoint(&self, cluster: Cluster) -> Option<&str> {
        match cluster {
            Cluster::Devnet => self.triton_devnet_rpc.as_deref(),
            Cluster::Mainnet => self.triton_mainnet_rpc.as_deref(),
        }
    }

    pub fn triton_token(&self, cluster: Cluster) -> Option<&str> {
        match cluster {
            Cluster::Devnet => self.triton_devnet_token.as_deref(),
            Cluster::Mainnet => self.triton_mainnet_token.as_deref(),
        }
    }
}

fn env_or_default(name: &str, fallback: &str) -> String {
    optional_env(name).unwrap_or_else(|| fallback.to_string())
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn secret(env_name: &str, key_name: &str) -> Option<String> {
    optional_env(env_name).or_else(|| {
        keyring::Entry::new(KEYRING_SERVICE, key_name)
            .ok()
            .and_then(|entry| entry.get_password().ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn bool_env(name: &str, fallback: bool) -> bool {
    optional_env(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(fallback)
}

fn number_env(name: &str, fallback: f64) -> f64 {
    optional_env(name)
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(fallback)
}
