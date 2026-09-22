//! Built-in check evaluators.
//!
//! Each [`crate::capability::CheckSpec`] has a `kind` field that
//! dispatches here. New kinds are added by extending [`evaluate_check`]
//! with a new match arm and updating [`known_kinds`].

use crate::{
    archive::ArchiveSource,
    capability::{CheckSpec, ResolvedConfig},
    module::{CheckCategory, CheckOutcome, CheckSeverity},
    path_guard,
};
use reqwest::Client;
use serde_json::Value as JsonValue;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
};

/// Catalogue of check kinds the runner knows how to evaluate. The
/// capability loader rejects specs that reference a kind outside this
/// set so the failure surfaces at startup.
pub fn known_kinds() -> &'static [&'static str] {
    &[
        "process-running",
        "file-exists",
        "file-absent",
        "archive-reachable",
        "archive-sha256",
    ]
}

fn param_string<'a>(check: &'a CheckSpec, name: &str) -> Option<&'a str> {
    check.params.get(name).and_then(|value| value.as_str())
}

fn param_config<'a>(
    check: &'a CheckSpec,
    name: &str,
    config: &'a ResolvedConfig,
) -> Option<String> {
    param_string(check, name)
        .and_then(|field| config.get_string(field))
}

/// Evaluate a single check and return the live outcome.
pub async fn evaluate_check(
    check: &CheckSpec,
    config: &ResolvedConfig,
    executable_directory: &Path,
) -> Result<CheckOutcome, String> {
    match check.kind.as_str() {
        "process-running" => Ok(evaluate_process_running(check, config)),
        "file-exists" => Ok(evaluate_file_exists(check, config, executable_directory, true)),
        "file-absent" => Ok(evaluate_file_exists(check, config, executable_directory, false)),
        "archive-reachable" => evaluate_archive_reachable(check, config).await,
        "archive-sha256" => evaluate_archive_sha256(check, config).await,
        other => Err(format!("Unknown check kind: {other}")),
    }
}

fn evaluate_process_running(
    check: &CheckSpec,
    config: &ResolvedConfig,
) -> CheckOutcome {
    let process_name = param_config(check, "processNameField", config)
        .or_else(|| param_string(check, "processName").map(str::to_owned));
    let Some(name) = process_name else {
        return unknown_check(check, "processNameField or processName is required");
    };
    let running = crate::process::is_process_running(&name);
    CheckOutcome {
        id: Some(check.id.clone()),
        category: parse_category(&check.category),
        severity: parse_severity(&check.severity),
        label: check.label.clone(),
        passed: running,
        detail: if running {
            None
        } else {
            Some(format!("Process `{name}` is not running."))
        },
    }
}

fn evaluate_file_exists(
    check: &CheckSpec,
    config: &ResolvedConfig,
    executable_directory: &Path,
    expect_exists: bool,
) -> CheckOutcome {
    let path_field = param_string(check, "pathField")
        .or_else(|| check.params.get("path").and_then(|value| value.as_str()));
    let resolved = match path_field {
        Some(field) => match config.get_string(field) {
            Some(path) => path_guard::safe_join_relative(executable_directory, &path)
                .map_err(|error| error),
            None => Err(format!(
                "file-{}: config field '{field}' is missing.",
                if expect_exists { "exists" } else { "absent" }
            )),
        },
        None => Err(format!(
            "file-{}: pathField or path is required.",
            if expect_exists { "exists" } else { "absent" }
        )),
    };
    let Ok(path) = resolved else {
        return error_check(check, resolved.unwrap_err());
    };
    let exists = path.is_file();
    let passed = exists == expect_exists;
    let detail = if passed {
        None
    } else if expect_exists {
        Some(format!("Expected file '{}' was not found.", path.display()))
    } else {
        Some(format!("Unexpected file present at '{}'.", path.display()))
    };
    CheckOutcome {
        id: Some(check.id.clone()),
        category: parse_category(&check.category),
        severity: parse_severity(&check.severity),
        label: check.label.clone(),
        passed,
        detail,
    }
}

async fn evaluate_archive_reachable(
    check: &CheckSpec,
    config: &ResolvedConfig,
) -> Result<CheckOutcome, String> {
    let url = param_config(check, "urlField", config)
        .or_else(|| param_string(check, "url").map(str::to_owned));
    let Some(url) = url else {
        return Ok(unknown_check(
            check,
            "urlField or url is required.",
        ));
    };
    let parsed = reqwest::Url::parse(&url)
        .map_err(|error| format!("archive-reachable: invalid URL '{url}': {error}"))?;
    if parsed.scheme() != "https" {
        return Ok(CheckOutcome {
            id: Some(check.id.clone()),
            category: parse_category(&check.category),
            severity: parse_severity(&check.severity),
            label: check.label.clone(),
            passed: false,
            detail: Some("archive-reachable: only HTTPS is allowed.".to_owned()),
        });
    }
    let client = Client::builder()
        .user_agent("Moddin-Desktop/0.1 (+https://github.com/petonexus/moddin-desktop)")
        .build()
        .map_err(|error| format!("archive-reachable: could not build client: {error}"))?;
    let response = client
        .head(parsed)
        .send()
        .await
        .map_err(|error| format!("archive-reachable: HEAD failed: {error}"))?;
    let passed = response.status().is_success();
    Ok(CheckOutcome {
        id: Some(check.id.clone()),
        category: parse_category(&check.category),
        severity: parse_severity(&check.severity),
        label: check.label.clone(),
        passed,
        detail: if passed {
            None
        } else {
            Some(format!("archive-reachable: HTTP {}", response.status()))
        },
    })
}

async fn evaluate_archive_sha256(
    check: &CheckSpec,
    config: &ResolvedConfig,
) -> Result<CheckOutcome, String> {
    let url = param_config(check, "urlField", config)
        .or_else(|| param_string(check, "url").map(str::to_owned));
    let expected = param_config(check, "expectedField", config)
        .or_else(|| param_string(check, "expected").map(str::to_owned));
    let (Some(url), Some(expected)) = (url, expected) else {
        return Ok(unknown_check(
            check,
            "urlField/url and expectedField/expected are required.",
        ));
    };
    let parsed = reqwest::Url::parse(&url)
        .map_err(|error| format!("archive-sha256: invalid URL '{url}': {error}"))?;
    let bytes = crate::archive::RemoteArchive::new(parsed.to_string())
        .with_host_allowlist(vec!["github.com".to_owned(), "objects.githubusercontent.com".to_owned()])
        .fetch()
        .await?;
    let (downloaded, computed) = bytes;
    let passed = computed.eq_ignore_ascii_case(&expected);
    Ok(CheckOutcome {
        id: Some(check.id.clone()),
        category: parse_category(&check.category),
        severity: parse_severity(&check.severity),
        label: check.label.clone(),
        passed,
        detail: if passed {
            Some(format!("SHA-256 verified: {computed}"))
        } else {
            Some(format!("SHA-256 mismatch (expected {expected}, got {computed})."))
        },
    })
}

fn parse_category(raw: &str) -> CheckCategory {
    match raw {
        "global" => CheckCategory::Global,
        "category" => CheckCategory::Category,
        _ => CheckCategory::ModuleSpecific,
    }
}

fn parse_severity(raw: &str) -> CheckSeverity {
    match raw {
        "warning" => CheckSeverity::Warning,
        "blocker" => CheckSeverity::Blocker,
        _ => CheckSeverity::Info,
    }
}

fn unknown_check(check: &CheckSpec, detail: &str) -> CheckOutcome {
    CheckOutcome {
        id: Some(check.id.clone()),
        category: parse_category(&check.category),
        severity: parse_severity(&check.severity),
        label: check.label.clone(),
        passed: false,
        detail: Some(detail.to_owned()),
    }
}

fn error_check(check: &CheckSpec, detail: String) -> CheckOutcome {
    CheckOutcome {
        id: Some(check.id.clone()),
        category: parse_category(&check.category),
        severity: parse_severity(&check.severity),
        label: check.label.clone(),
        passed: false,
        detail: Some(detail),
    }
}

#[allow(dead_code)]
fn _typecheck_reserved_imports(
    _b: BTreeMap<String, JsonValue>,
    _p: &Path,
) -> std::io::Result<()> {
    let _ = fs::metadata(_p);
    Ok(())
}