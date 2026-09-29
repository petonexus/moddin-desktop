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
    process::HideConsole,
};
use reqwest::Client;
use serde_json::Value as JsonValue;
use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};

/// Per-execution cache of exe `FileVersion` probes, keyed by absolute
/// path. A cached `None` means the probe already ran and found no
/// version, so the slow PowerShell lookup happens at most once per exe
/// per evaluation run.
pub type ExeVersionCache = BTreeMap<PathBuf, Option<String>>;

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
        "exe-version",
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
    exe_versions: &mut ExeVersionCache,
) -> Result<CheckOutcome, String> {
    match check.kind.as_str() {
        "process-running" => Ok(evaluate_process_running(check, config)),
        "file-exists" => Ok(evaluate_file_exists(check, config, executable_directory, true)),
        "file-absent" => Ok(evaluate_file_exists(check, config, executable_directory, false)),
        "archive-reachable" => evaluate_archive_reachable(check, config).await,
        "archive-sha256" => evaluate_archive_sha256(check, config).await,
        "exe-version" => Ok(evaluate_exe_version(
            check,
            config,
            executable_directory,
            exe_versions,
        )),
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

/// Accepted-version window for the `exe-version` check.
#[derive(Debug, Clone, Default)]
struct ExeVersionConstraints {
    min: Option<String>,
    max: Option<String>,
    blocked: Vec<String>,
    exact: Vec<String>,
}

fn exe_version_constraints(check: &CheckSpec) -> ExeVersionConstraints {
    ExeVersionConstraints {
        min: param_string(check, "minVersion").map(str::to_owned),
        max: param_string(check, "maxVersion").map(str::to_owned),
        blocked: param_string_list(check, "blockedVersions"),
        exact: param_string_list(check, "exactVersions"),
    }
}

fn param_string_list(check: &CheckSpec, name: &str) -> Vec<String> {
    check
        .params
        .get(name)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Evaluate `exe-version`: read the game executable's `FileVersion`
/// and compare it against the accepted window (`minVersion` /
/// `maxVersion` inclusive, `blockedVersions` rejected, `exactVersions`
/// whitelist when non-empty).
pub(crate) fn evaluate_exe_version(
    check: &CheckSpec,
    config: &ResolvedConfig,
    executable_directory: &Path,
    exe_versions: &mut ExeVersionCache,
) -> CheckOutcome {
    evaluate_exe_version_with_probe(
        check,
        config,
        executable_directory,
        exe_versions,
        &probe_exe_file_version,
    )
}

/// [`evaluate_exe_version`] with an injectable probe so tests do not
/// need a real versioned executable on disk.
fn evaluate_exe_version_with_probe(
    check: &CheckSpec,
    config: &ResolvedConfig,
    executable_directory: &Path,
    exe_versions: &mut ExeVersionCache,
    probe: &dyn Fn(&Path) -> Option<String>,
) -> CheckOutcome {
    let resolved = if let Some(field) = param_string(check, "pathField") {
        match config.get_string(field) {
            Some(path) => path_guard::safe_join_relative(executable_directory, &path),
            None => Err(format!("exe-version: config field '{field}' is missing.")),
        }
    } else if let Some(path) = param_string(check, "path") {
        path_guard::safe_join_relative(executable_directory, path)
    } else {
        Err("exe-version: pathField or path is required.".to_owned())
    };
    let Ok(path) = resolved else {
        return error_check(check, resolved.unwrap_err());
    };

    let detected = match exe_versions.get(&path) {
        Some(cached) => cached.clone(),
        None => {
            let version = probe(&path);
            exe_versions.insert(path.clone(), version.clone());
            version
        }
    };

    exe_version_outcome(check, &path, detected.as_deref(), &exe_version_constraints(check))
}

/// Pure decision core of `exe-version`, split from the probe so it is
/// unit-testable without PowerShell or a real PE binary.
fn exe_version_outcome(
    check: &CheckSpec,
    path: &Path,
    detected: Option<&str>,
    constraints: &ExeVersionConstraints,
) -> CheckOutcome {
    let mut outcome = base_outcome(check);
    let Some(detected) = detected else {
        outcome.passed = false;
        outcome.detail = Some(format!(
            "exe-version: could not determine the FileVersion of '{}'.",
            path.display()
        ));
        return outcome;
    };
    let Some(actual) = crate::updates::parse_version(detected) else {
        outcome.passed = false;
        outcome.detail = Some(format!(
            "exe-version: FileVersion '{detected}' of '{}' is not a parseable version.",
            path.display()
        ));
        return outcome;
    };

    let mut problems: Vec<String> = Vec::new();
    if let Some(min) = constraints.min.as_deref() {
        match crate::updates::parse_version(min) {
            Some(minimum) if crate::updates::compare_versions(&actual, &minimum) == Ordering::Less => {
                problems.push(format!("below the minimum {min}"))
            }
            Some(_) => {}
            None => problems.push(format!("minVersion '{min}' is not a valid version")),
        }
    }
    if let Some(max) = constraints.max.as_deref() {
        match crate::updates::parse_version(max) {
            Some(maximum) if crate::updates::compare_versions(&actual, &maximum) == Ordering::Greater => {
                problems.push(format!("above the maximum {max}"))
            }
            Some(_) => {}
            None => problems.push(format!("maxVersion '{max}' is not a valid version")),
        }
    }
    for blocked in &constraints.blocked {
        match crate::updates::parse_version(blocked) {
            Some(blocked_version)
                if crate::updates::compare_version_cores(&actual, &blocked_version)
                    == Ordering::Equal =>
            {
                problems.push(format!("blocked version {blocked}"))
            }
            _ => {}
        }
    }
    if !constraints.exact.is_empty()
        && !constraints
            .exact
            .iter()
            .filter_map(|candidate| crate::updates::parse_version(candidate))
            .any(|candidate| {
                crate::updates::compare_version_cores(&actual, &candidate) == Ordering::Equal
            })
    {
        problems.push(format!(
            "not one of the exact versions [{}]",
            constraints.exact.join(", ")
        ));
    }

    outcome.passed = problems.is_empty();
    outcome.detail = if problems.is_empty() {
        Some(format!(
            "exe-version: '{}' reports FileVersion {detected}.",
            path.display()
        ))
    } else {
        Some(format!(
            "exe-version: '{}' reports FileVersion {detected}, which is {}.",
            path.display(),
            problems.join("; ")
        ))
    };
    outcome
}

/// Outcome scaffold carrying the check's identity fields.
fn base_outcome(check: &CheckSpec) -> CheckOutcome {
    CheckOutcome {
        id: Some(check.id.clone()),
        category: parse_category(&check.category),
        severity: parse_severity(&check.severity),
        label: check.label.clone(),
        passed: false,
        detail: None,
    }
}

/// Reads the Windows `FileVersion` of `path` via PowerShell
/// `(Get-Item ...).VersionInfo.FileVersion` — the same channel the
/// rest of the codebase uses for version probes. `None` means the
/// version could not be determined (missing file, no version
/// resource, PowerShell unavailable).
fn probe_exe_file_version(path: &Path) -> Option<String> {
    let path_text = path.to_string_lossy().replace('\'', "''");
    let command =
        format!("(Get-Item -LiteralPath '{path_text}').VersionInfo.FileVersion");
    let output = std::process::Command::new("powershell.exe")
        .hide_console()
        .args(["-NoProfile", "-Command", &command])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn exe_version_check(params: BTreeMap<String, JsonValue>) -> CheckSpec {
        CheckSpec {
            id: "exe-version".to_owned(),
            label: "Game exe version".to_owned(),
            kind: "exe-version".to_owned(),
            params,
            severity: "blocker".to_owned(),
            category: "modulespecific".to_owned(),
            description: None,
        }
    }

    fn outcome_with_version(
        check: &CheckSpec,
        detected: Option<&str>,
    ) -> CheckOutcome {
        exe_version_outcome(
            check,
            Path::new("C:/Games/Example/Game.exe"),
            detected,
            &exe_version_constraints(check),
        )
    }

    #[test]
    fn exe_version_passes_inside_min_max_window() {
        let check = exe_version_check(BTreeMap::from([
            ("path".to_owned(), json!("Game.exe")),
            ("minVersion".to_owned(), json!("1.16.0")),
            ("maxVersion".to_owned(), json!("1.17.9")),
        ]));
        assert!(outcome_with_version(&check, Some("1.16.0.0")).passed);
        assert!(outcome_with_version(&check, Some("1.17.9")).passed);
        assert!(outcome_with_version(&check, Some("1.17.8.9")).passed);
        assert!(outcome_with_version(&check, Some("v1.17")).passed);
        // Windows FileVersion often carries a " (WinBuild.…)" suffix;
        // the tolerant parser must still place the version in range.
        assert!(outcome_with_version(&check, Some("1.17.3.42 (WinBuild.160101.0800)")).passed);
        let below = outcome_with_version(&check, Some("1.15.9.9"));
        assert!(!below.passed);
        assert!(below.detail.expect("detail").contains("below the minimum"));
        let above = outcome_with_version(&check, Some("1.18.0"));
        assert!(!above.passed);
        assert!(above.detail.expect("detail").contains("above the maximum"));
        let just_above = outcome_with_version(&check, Some("1.17.9.1"));
        assert!(!just_above.passed);
        assert!(just_above
            .detail
            .expect("detail")
            .contains("above the maximum"));
    }

    #[test]
    fn exe_version_honours_blocked_and_exact_lists() {
        let blocked = exe_version_check(BTreeMap::from([
            ("path".to_owned(), json!("Game.exe")),
            (
                "blockedVersions".to_owned(),
                json!(["1.16.3.0", "1.16.3.1"]),
            ),
        ]));
        assert!(outcome_with_version(&blocked, Some("1.16.2.0")).passed);
        let hit = outcome_with_version(&blocked, Some("1.16.3.0"));
        assert!(!hit.passed);
        assert!(hit.detail.expect("detail").contains("blocked version"));
        // The numeric quad identifies the build: a WinBuild annotation
        // on the FileVersion must not dodge a blocked entry.
        let annotated = outcome_with_version(&blocked, Some("1.16.3.0 (WinBuild.160101.0800)"));
        assert!(!annotated.passed, "core equality must still block");

        let exact = exe_version_check(BTreeMap::from([
            ("path".to_owned(), json!("Game.exe")),
            ("exactVersions".to_owned(), json!(["2.0.0", "2.1.0"])),
        ]));
        assert!(outcome_with_version(&exact, Some("2.1.0.0")).passed);
        assert!(outcome_with_version(&exact, Some("2.1.0 (WinBuild.160101.0800)")).passed);
        let other = outcome_with_version(&exact, Some("2.2.0"));
        assert!(!other.passed);
        assert!(other.detail.expect("detail").contains("exact versions"));
    }

    #[test]
    fn exe_version_fails_on_missing_or_unparseable_version() {
        let check = exe_version_check(BTreeMap::from([
            ("path".to_owned(), json!("Game.exe")),
            ("minVersion".to_owned(), json!("1.0.0")),
        ]));
        let missing = outcome_with_version(&check, None);
        assert!(!missing.passed);
        assert!(missing.detail.expect("detail").contains("could not determine"));
        let unparsable = outcome_with_version(&check, Some("not-a-version"));
        assert!(!unparsable.passed);
        assert!(unparsable.detail.expect("detail").contains("not a parseable version"));
    }

    #[test]
    fn exe_version_reports_invalid_constraints() {
        let check = exe_version_check(BTreeMap::from([
            ("path".to_owned(), json!("Game.exe")),
            ("minVersion".to_owned(), json!("banana")),
        ]));
        let outcome = outcome_with_version(&check, Some("1.2.3"));
        assert!(!outcome.passed);
        assert!(outcome
            .detail
            .expect("detail")
            .contains("minVersion 'banana' is not a valid version"));
    }

    #[test]
    fn exe_version_requires_path_param() {
        let check = exe_version_check(BTreeMap::from([]));
        let outcome = evaluate_exe_version_with_probe(
            &check,
            &ResolvedConfig::default(),
            Path::new("C:/Games/Example"),
            &mut ExeVersionCache::new(),
            &|_| Some("1.0.0".to_owned()),
        );
        assert!(!outcome.passed);
        assert!(outcome
            .detail
            .expect("detail")
            .contains("pathField or path is required"));
    }

    #[test]
    fn exe_version_resolves_path_field_from_config() {
        let check = exe_version_check(BTreeMap::from([
            ("pathField".to_owned(), json!("gameExePath")),
            ("minVersion".to_owned(), json!("1.0.0")),
        ]));
        let mut config = ResolvedConfig::default();
        config
            .values
            .insert("gameExePath".to_owned(), json!("Binaries/Game.exe"));
        let outcome = evaluate_exe_version_with_probe(
            &check,
            &config,
            Path::new("C:/Games/Example"),
            &mut ExeVersionCache::new(),
            &|path| {
                assert_eq!(
                    path,
                    Path::new("C:/Games/Example/Binaries/Game.exe")
                );
                Some("1.4.0".to_owned())
            },
        );
        assert!(outcome.passed);
    }

    #[test]
    fn exe_version_caches_probe_per_path_within_a_run() {
        let check = exe_version_check(BTreeMap::from([
            ("path".to_owned(), json!("Game.exe")),
            ("minVersion".to_owned(), json!("1.0.0")),
        ]));
        let calls = AtomicUsize::new(0);
        let probe = |path: &Path| {
            assert_eq!(path, Path::new("C:/Games/Example/Game.exe"));
            calls.fetch_add(1, AtomicOrdering::SeqCst);
            Some("1.4.0".to_owned())
        };
        let mut cache = ExeVersionCache::new();
        for _ in 0..3 {
            let outcome = evaluate_exe_version_with_probe(
                &check,
                &ResolvedConfig::default(),
                Path::new("C:/Games/Example"),
                &mut cache,
                &probe,
            );
            assert!(outcome.passed);
        }
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn probe_reads_a_real_exe_file_version() {
        // Exercises the actual PowerShell probe end to end. Skipped on
        // machines without the system binary (non-Windows dev boxes).
        let system_root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let exe = system_root.join("System32").join("notepad.exe");
        if !exe.is_file() {
            return;
        }
        let version = probe_exe_file_version(&exe).expect("notepad.exe carries a FileVersion");
        let parsed = crate::updates::parse_version(&version)
            .expect("the probed FileVersion parses with the shared comparator");
        assert!(!parsed.core.is_empty());
    }

    #[test]
    fn exe_version_accepts_four_component_file_versions() {
        // Windows FileVersion is typically a 4-component quad
        // ("1.16.0.0"); the tolerant parser must accept it and compare
        // it naturally against 2–3 component bounds.
        let check = exe_version_check(BTreeMap::from([
            ("path".to_owned(), json!("Game.exe")),
            ("minVersion".to_owned(), json!("1.16.0")),
            ("maxVersion".to_owned(), json!("1.16.99")),
        ]));
        assert!(outcome_with_version(&check, Some("1.16.0.0")).passed);
        assert!(!outcome_with_version(&check, Some("10.0.0.0")).passed);
    }
}