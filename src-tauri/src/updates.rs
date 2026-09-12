use reqwest::Client;
use serde::{Deserialize, Serialize};

const GITHUB_API_RELEASE_PREFIX: &str = "https://api.github.com/repos/";
const GITHUB_API_RELEASE_SUFFIX: &str = "/releases/latest";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleUpdateRequest {
    pub current_version: Option<String>,
    pub update_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleUpdateResult {
    pub status: String,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub release_url: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: Option<String>,
}

fn unsupported(detail: &str, current_version: Option<String>) -> ModuleUpdateResult {
    ModuleUpdateResult {
        status: "unavailable".to_owned(),
        current_version,
        latest_version: None,
        release_url: None,
        detail: Some(detail.to_owned()),
    }
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .take(3)
        .collect()
}

fn version_is_newer(current: &str, latest: &str) -> bool {
    let current_parts = version_parts(current);
    let latest_parts = version_parts(latest);
    if current_parts.is_empty() || latest_parts.is_empty() {
        return false;
    }

    for index in 0..current_parts.len().max(latest_parts.len()) {
        let current_part = current_parts.get(index).copied().unwrap_or(0);
        let latest_part = latest_parts.get(index).copied().unwrap_or(0);
        if latest_part != current_part {
            return latest_part > current_part;
        }
    }

    false
}

#[tauri::command]
pub async fn check_module_update(
    request: ModuleUpdateRequest,
) -> Result<ModuleUpdateResult, String> {
    let Some(update_url) = request.update_url.as_deref().map(str::trim).filter(|url| !url.is_empty())
    else {
        return Ok(unsupported(
            "This module recipe does not define an update source.",
            request.current_version,
        ));
    };

    if !update_url.starts_with(GITHUB_API_RELEASE_PREFIX)
        || !update_url.ends_with(GITHUB_API_RELEASE_SUFFIX)
    {
        return Err(
            "Module update sources must use the official GitHub latest-release API endpoint."
                .to_owned(),
        );
    }

    let client = Client::builder()
        .user_agent("Moddin/0.1 (+https://github.com/marcoasjunior/moddin)")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Could not create update checker: {error}"))?;

    let release = client
        .get(update_url)
        .send()
        .await
        .map_err(|error| format!("Could not check module updates: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Update source returned an error: {error}"))?
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("Could not parse module update information: {error}"))?;

    let current_version = request.current_version;
    let status = match current_version.as_deref() {
        Some(current) if version_is_newer(current, &release.tag_name) => "available",
        Some(_) => "current",
        None => "unknown",
    };

    Ok(ModuleUpdateResult {
        status: status.to_owned(),
        current_version,
        latest_version: Some(release.tag_name),
        release_url: release.html_url,
        detail: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_newer_release() {
        assert!(version_is_newer("0.9.4", "v0.9.5"));
        assert!(!version_is_newer("0.9.4", "v0.9.4"));
        assert!(!version_is_newer("unknown", "v0.9.5"));
    }
}
