use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

const GITHUB_API_RELEASE_PREFIX: &str = "https://api.github.com/repos/";
const GITHUB_API_RELEASE_SUFFIX: &str = "/releases/latest";
const GITHUB_API_RELEASE_LIST_SUFFIX: &str = "/releases?per_page=1";

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedVersion {
    core: Vec<u64>,
    prerelease: Option<String>,
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

fn parse_version(value: &str) -> Option<ParsedVersion> {
    let value = value.trim();
    let first_digit = value.find(|character: char| character.is_ascii_digit())?;
    let value = value[first_digit..].split('+').next().unwrap_or_default();
    let core_end = value
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(value.len());
    let core_text = value[..core_end].trim_end_matches('.');
    if core_text.is_empty() {
        return None;
    }

    let mut core = Vec::new();
    for part in core_text.split('.') {
        if part.is_empty() || !part.chars().all(|character| character.is_ascii_digit()) {
            return None;
        }
        core.push(part.parse::<u64>().ok()?);
    }
    if core.is_empty() {
        return None;
    }

    let prerelease = value[core_end..]
        .trim_start_matches(['-', '_', '.'])
        .trim()
        .to_ascii_lowercase();
    if !prerelease.is_ascii() {
        return None;
    }
    let prerelease = (!prerelease.is_empty()).then_some(prerelease);

    Some(ParsedVersion { core, prerelease })
}

fn compare_digit_runs(left: &str, right: &str) -> Ordering {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    let left = if left.is_empty() { "0" } else { left };
    let right = if right.is_empty() { "0" } else { right };

    left.len()
        .cmp(&right.len())
        .then_with(|| left.cmp(right))
}

fn compare_prerelease_natural(left: &str, right: &str) -> Ordering {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left_bytes.len() && right_index < right_bytes.len() {
        let left_digit = left_bytes[left_index].is_ascii_digit();
        let right_digit = right_bytes[right_index].is_ascii_digit();

        if left_digit && right_digit {
            let left_start = left_index;
            let right_start = right_index;
            while left_index < left_bytes.len() && left_bytes[left_index].is_ascii_digit() {
                left_index += 1;
            }
            while right_index < right_bytes.len() && right_bytes[right_index].is_ascii_digit() {
                right_index += 1;
            }
            let ordering = compare_digit_runs(
                &left[left_start..left_index],
                &right[right_start..right_index],
            );
            if ordering != Ordering::Equal {
                return ordering;
            }
            continue;
        }

        if left_digit != right_digit {
            return if left_digit {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }

        let left_start = left_index;
        let right_start = right_index;
        while left_index < left_bytes.len() && !left_bytes[left_index].is_ascii_digit() {
            left_index += 1;
        }
        while right_index < right_bytes.len() && !right_bytes[right_index].is_ascii_digit() {
            right_index += 1;
        }
        let ordering = left[left_start..left_index].cmp(&right[right_start..right_index]);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    left_bytes.len().cmp(&right_bytes.len())
}

fn compare_versions(left: &ParsedVersion, right: &ParsedVersion) -> Ordering {
    for index in 0..left.core.len().max(right.core.len()) {
        let left_part = left.core.get(index).copied().unwrap_or(0);
        let right_part = right.core.get(index).copied().unwrap_or(0);
        if left_part != right_part {
            return left_part.cmp(&right_part);
        }
    }

    match (&left.prerelease, &right.prerelease) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => compare_prerelease_natural(left, right),
    }
}

fn version_is_newer(current: &str, latest: &str) -> bool {
    let Some(current) = parse_version(current) else {
        return false;
    };
    let Some(latest) = parse_version(latest) else {
        return false;
    };

    compare_versions(&latest, &current) == Ordering::Greater
}

#[tauri::command]
pub async fn check_module_update(
    request: ModuleUpdateRequest,
) -> Result<ModuleUpdateResult, String> {
    let Some(update_url) = request
        .update_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    else {
        return Ok(unsupported(
            "This module recipe does not define an update source.",
            request.current_version,
        ));
    };

    let is_latest_endpoint = update_url.ends_with(GITHUB_API_RELEASE_SUFFIX);
    let is_release_list_endpoint = update_url.ends_with(GITHUB_API_RELEASE_LIST_SUFFIX);
    if !update_url.starts_with(GITHUB_API_RELEASE_PREFIX)
        || (!is_latest_endpoint && !is_release_list_endpoint)
    {
        return Err(
            "Module update sources must use an official GitHub release API endpoint."
                .to_owned(),
        );
    }

    let client = Client::builder()
        .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Could not create update checker: {error}"))?;

    let response = client
        .get(update_url)
        .send()
        .await
        .map_err(|error| format!("Could not check module updates: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Update source returned an error: {error}"))?;

    let release = if is_latest_endpoint {
        response
            .json::<GithubRelease>()
            .await
            .map_err(|error| format!("Could not parse module update information: {error}"))?
    } else {
        let releases = response
            .json::<Vec<GithubRelease>>()
            .await
            .map_err(|error| format!("Could not parse module update information: {error}"))?;
        releases
            .into_iter()
            .next()
            .ok_or_else(|| "The GitHub release list is empty.".to_owned())?
    };

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
    fn detects_newer_numeric_release() {
        assert!(version_is_newer("0.9.4", "v0.9.5"));
        assert!(!version_is_newer("0.9.4", "v0.9.4"));
        assert!(!version_is_newer("0.9.5", "v0.9.4"));
        assert!(!version_is_newer("unknown", "v0.9.5"));
    }

    #[test]
    fn stable_release_is_newer_than_same_core_prerelease() {
        assert!(version_is_newer("0.9.5-beta.2", "v0.9.5"));
        assert!(!version_is_newer("0.9.5", "v0.9.5-rc.1"));
    }

    #[test]
    fn prerelease_numbers_use_natural_ordering() {
        assert!(version_is_newer("0.7.7-pre5", "v0.7.7-pre10"));
        assert!(version_is_newer("1.0.0-beta.2", "1.0.0-beta.10"));
        assert!(version_is_newer("1.0.0-beta.10", "1.0.0-rc.1"));
    }

    #[test]
    fn ignores_build_metadata_and_accepts_prefixed_tags() {
        assert!(!version_is_newer("1.2.3+local.1", "v1.2.3+build.9"));
        assert!(version_is_newer("0.9.4", "OptiScaler_v0.9.5"));
    }

    #[test]
    fn rejects_non_ascii_prerelease_suffixes() {
        assert!(parse_version("1.0.0-béta.1").is_none());
        assert!(!version_is_newer("1.0.0", "1.0.0-béta.2"));
    }
}
