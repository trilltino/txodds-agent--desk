//! TxLINE documented HTTP API helpers.
//!
//! This module mirrors the public OpenAPI/data docs while keeping credentials
//! in Rust. Tauri commands call these helpers instead of building ad hoc URLs in
//! the webview.

use reqwest::Client;
use serde_json::Value;

use crate::config::AppConfig;
use crate::error::AppError;

pub async fn authenticated_get(
    client: &Client,
    config: &AppConfig,
    path: &str,
    query: Vec<(&str, String)>,
) -> Result<Value, AppError> {
    let path = normalize_data_path(path)?;
    let jwt = config
        .txline_guest_jwt
        .as_deref()
        .ok_or_else(|| AppError::Config("TXLINE_GUEST_JWT missing".to_string()))?;
    let token = config
        .txline_api_token
        .as_deref()
        .ok_or_else(|| AppError::Config("TXLINE_API_TOKEN missing".to_string()))?;
    let url = format!(
        "{}/{}",
        config.txline_api_origin.trim_end_matches('/'),
        path
    );
    let request = client
        .get(url)
        .bearer_auth(jwt)
        .header("X-Api-Token", token);
    let request = if query.is_empty() {
        request
    } else {
        request.query(&query)
    };
    let response = request.send().await?.error_for_status()?;
    Ok(response.json::<Value>().await?)
}

pub fn normalize_data_path(path: &str) -> Result<String, AppError> {
    let (path_part, query_part) = path
        .trim()
        .split_once('?')
        .map(|(path, query)| (path, Some(query)))
        .unwrap_or((path.trim(), None));
    if path_part.contains("://")
        || path_part.contains('\\')
        || path_part.contains("..")
        || path_part.contains("//")
        || path_part.contains('#')
    {
        return Err(AppError::InvalidInput(
            "TxLINE path contains disallowed characters".to_string(),
        ));
    }

    let mut clean = path_part.trim_matches('/').to_string();
    if !clean.starts_with("api/") {
        clean = format!("api/{clean}");
    }
    if !is_allowed_data_path(&clean) {
        return Err(AppError::InvalidInput(format!(
            "TxLINE path {clean} is not in the documented data allowlist"
        )));
    }

    if let Some(query) = query_part {
        if query.contains('#') || query.contains("://") {
            return Err(AppError::InvalidInput(
                "TxLINE query contains disallowed characters".to_string(),
            ));
        }
        if query.trim().is_empty() {
            Ok(clean)
        } else {
            Ok(format!("{clean}?{query}"))
        }
    } else {
        Ok(clean)
    }
}

fn is_allowed_data_path(path: &str) -> bool {
    matches!(
        path,
        "api/fixtures/snapshot"
            | "api/fixtures/validation"
            | "api/fixtures/batch-validation"
            | "api/odds/validation"
            | "api/scores/stat-validation"
    ) || has_allowed_prefix(
        path,
        &[
            "api/fixtures/updates/",
            "api/odds/snapshot/",
            "api/odds/updates/",
            "api/scores/snapshot/",
            "api/scores/updates/",
            "api/scores/historical/",
        ],
    )
}

fn has_allowed_prefix(path: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| path.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_documented_data_paths() {
        assert_eq!(
            normalize_data_path("scores/snapshot/17952170?asOf=123").unwrap(),
            "api/scores/snapshot/17952170?asOf=123"
        );
        assert_eq!(
            normalize_data_path("/api/odds/stream")
                .unwrap_err()
                .to_string(),
            "invalid input: TxLINE path api/odds/stream is not in the documented data allowlist"
        );
    }

    #[test]
    fn rejects_external_or_parent_paths() {
        assert!(normalize_data_path("https://txline.txodds.com/api/scores/stream").is_err());
        assert!(normalize_data_path("../api/token/activate").is_err());
        assert!(normalize_data_path("api/token/activate").is_err());
    }
}
