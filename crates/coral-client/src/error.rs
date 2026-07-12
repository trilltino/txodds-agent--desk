#[derive(Debug, thiserror::Error)]
pub enum CoralClientError {
    #[error("CORAL_CONNECTION_URL not set — coral-server must launch this participant")]
    NoConnectionUrl,
    #[error("failed to connect to CoralOS: {0}")]
    Connect(String),
    #[error("CoralOS MCP call failed: {0}")]
    Mcp(String),
}
