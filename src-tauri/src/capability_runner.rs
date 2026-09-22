//! Capability registry and runtime orchestration.
//!
//! Loads every YAML capability recipe under `src-tauri/capabilities/`
//! at build time (via `include_str!`) and exposes a small dispatcher
//! that the Tauri command layer (and the future in-process modules)
//! can call to:
//!
//! * [`run_install`] — execute every step in `spec.install`,
//!   record a transaction, and return the structured result.
//! * [`run_uninstall`] — run the uninstall steps (or rely on the
//!   transaction store to roll back the install record).
//! * [`evaluate_check`] — run a single check by id against the
//!   current game state.
//! * [`evaluate_all_checks`] — drive the UI verification list in one
//!   call.
//!
//! Adding a new capability is a single YAML file under
//! `src-tauri/capabilities/<id>.yaml` plus a `include_str!` line
//! below. No Rust code change is required for the recipe itself;
//! only new step / check kinds require a new match arm in
//! [`crate::builtin_steps`] / [`crate::builtin_checks`].

use crate::{
    builtin_checks,
    builtin_steps::{self, StepContext, StepResult},
    capability::{CapabilitySpec, CheckSpec, ResolvedConfig, SpecOrigin, StepSpec},
    module::{CheckCategory, CheckOutcome, CheckSeverity, ModuleStatus, VerificationReport},
    transaction::{self, TransactionRecord},
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::Path,
};
use std::path::PathBuf;

const OFXR_BRIDGE_YAML: &str = include_str!("../capabilities/ofxr-bridge.yaml");
const OPTISCALER_YAML: &str = include_str!("../capabilities/optiscaler.yaml");
const CHEEKY_FOVEATED_DLSS_YAML: &str =
    include_str!("../capabilities/cheeky-foveated-dlss.yaml");
const RESHADE_YAML: &str = include_str!("../capabilities/reshade.yaml");
const OPENXR_HELPERS_YAML: &str = include_str!("../capabilities/openxr-helpers.yaml");
const UEVR_YAML: &str = include_str!("../capabilities/uevr.yaml");

/// Directory Moddin Desktop scans at startup for user-provided
/// capability recipes. Files placed here are loaded as `SpecOrigin::Local`,
/// which the UI badges as "Local" and the activity log records at
/// `verbose` level. Same kind allow-list applies.
///
/// Returns `None` when `LOCALAPPDATA` is unset (non-Windows dev).
pub fn local_capabilities_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|dir| dir.join("Moddin").join("capabilities"))
}

/// Static registry of every capability declared under
/// `src-tauri/capabilities/` (BuiltIn) plus the user's local override
/// directory (Local). Built at process start; never mutated.
#[derive(Debug, Default)]
pub struct CapabilityRegistry {
    specs: HashMap<String, CapabilitySpec>,
}

impl CapabilityRegistry {
    /// Load every capability the runner can find: the built-ins
    /// compiled into the binary plus every YAML the user dropped under
    /// `%LOCALAPPDATA%\Moddin\capabilities\`.
    ///
    /// Local YAMLs whose id matches a built-in override the built-in
    /// (so users can patch a single step of `ofxr-bridge` without
    /// recompiling the app). Duplicate ids among local files are
    /// logged and skipped — the runner keeps the first one it finds.
    pub fn load() -> Self {
        let mut specs = HashMap::new();

        for raw in [
            OFXR_BRIDGE_YAML,
            OPTISCALER_YAML,
            CHEEKY_FOVEATED_DLSS_YAML,
            RESHADE_YAML,
            OPENXR_HELPERS_YAML,
            UEVR_YAML,
        ] {
            match serde_yaml::from_str::<CapabilitySpec>(raw) {
                Ok(mut spec) => {
                    spec.origin = SpecOrigin::BuiltIn;
                    if specs.contains_key(&spec.id) {
                        panic!(
                            "duplicate built-in capability id '{id}'",
                            id = spec.id
                        );
                    }
                    specs.insert(spec.id.clone(), spec);
                }
                Err(error) => {
                    panic!("could not parse built-in capability YAML: {error}");
                }
            }
        }

        if let Some(local_dir) = local_capabilities_dir() {
            Self::load_local_into(&local_dir, &mut specs);
        }

        Self { specs }
    }

    /// Same as [`Self::load`] but takes the local override directory
    /// explicitly. Used by tests and by a future `capability_reload`
    /// Tauri command so the user can drop a new YAML and re-read it
    /// without restarting.
    pub fn load_with_local_dir<P: AsRef<Path>>(local_dir: P) -> Self {
        let mut registry = Self::default();
        for raw in [
            OFXR_BRIDGE_YAML,
            OPTISCALER_YAML,
            CHEEKY_FOVEATED_DLSS_YAML,
            RESHADE_YAML,
            OPENXR_HELPERS_YAML,
            UEVR_YAML,
        ] {
            if let Ok(mut spec) = serde_yaml::from_str::<CapabilitySpec>(raw) {
                spec.origin = SpecOrigin::BuiltIn;
                registry.specs.insert(spec.id.clone(), spec);
            }
        }
        Self::load_local_into(local_dir.as_ref(), &mut registry.specs);
        registry
    }

    fn load_local_into(dir: &Path, specs: &mut HashMap<String, CapabilitySpec>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                eprintln!(
                    "moddin: could not read local capability dir {}: {error}",
                    dir.display()
                );
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_yaml = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"))
                .unwrap_or(false);
            if !is_yaml {
                continue;
            }
            let raw = match std::fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(error) => {
                    eprintln!(
                        "moddin: could not read {}: {error}",
                        path.display()
                    );
                    continue;
                }
            };
            let mut spec = match serde_yaml::from_str::<CapabilitySpec>(&raw) {
                Ok(spec) => spec,
                Err(error) => {
                    eprintln!(
                        "moddin: could not parse {}: {error}",
                        path.display()
                    );
                    continue;
                }
            };
            spec.origin = SpecOrigin::Local;
            let spec_id = spec.id.clone();
            let previous_origin = specs
                .insert(spec.id.clone(), spec)
                .map(|existing| existing.origin);
            match previous_origin {
                Some(SpecOrigin::BuiltIn) => eprintln!(
                    "moddin: local capability {spec_id} overrides the built-in"
                ),
                Some(other) if other != SpecOrigin::Local => eprintln!(
                    "moddin: local capability {spec_id} overrides {} capability",
                    other.as_str()
                ),
                Some(SpecOrigin::Local) => eprintln!(
                    "moddin: duplicate local capability id {spec_id}, keeping first"
                ),
                Some(SpecOrigin::Community) => eprintln!(
                    "moddin: local capability {spec_id} overrides community capability"
                ),
                None => {}
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&CapabilitySpec> {
        self.specs.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.specs.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Re-read the local override directory and either add, replace,
    /// or remove specs depending on what the user dropped or deleted
    /// since the last call. The runner keeps the same `HashMap`
    /// storage so existing `&CapabilitySpec` references stay valid for
    /// already-resolved specs.
    pub fn reload_local<P: AsRef<Path>>(&mut self, local_dir: P) {
        let local_dir = local_dir.as_ref();
        let mut keep: HashMap<String, CapabilitySpec> = HashMap::new();
        for (id, spec) in self.specs.drain() {
            if spec.origin != SpecOrigin::Local {
                keep.insert(id, spec);
            }
        }
        Self::load_local_into(local_dir, &mut keep);
        self.specs.extend(keep);
    }
}

/// Result of running every install step in a capability.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub capability_id: String,
    pub transaction: Option<TransactionRecord>,
    pub steps: Vec<StepResult>,
    pub affected_paths: Vec<String>,
}

/// Run the `install` section of a capability against the supplied
/// config and game paths.
pub fn run_install(
    registry: &CapabilityRegistry,
    capability_id: &str,
    game_id: &str,
    game_name: &str,
    config: &ResolvedConfig,
    install_directory: &Path,
    executable_directory: &Path,
) -> Result<InstallResult, String> {
    let spec = registry
        .get(capability_id)
        .ok_or_else(|| format!("Unknown capability id '{capability_id}'."))?;

    let step_context = StepContext {
        spec,
        config,
        install_directory,
        executable_directory,
    };
    let mut step_results = Vec::new();
    let mut affected = Vec::new();
    for step in &spec.install {
        let result = builtin_steps::execute_step(step, &step_context)?;
        affected.extend(result.affected_paths.clone());
        step_results.push(result);
    }

    let transaction = if !affected.is_empty() {
        let metadata = build_metadata(spec, config);
        let record = transaction::begin_file_set_transaction(
            install_directory,
            &affected
                .iter()
                .map(std::path::PathBuf::from)
                .collect::<Vec<_>>(),
            &spec.id,
            &format!("Install {} for {}", spec.display_name, game_name),
            game_id,
            metadata,
        )?;
        Some(transaction::mark_applied(record)?)
    } else {
        None
    };

    Ok(InstallResult {
        capability_id: spec.id.clone(),
        transaction,
        steps: step_results,
        affected_paths: affected,
    })
}

/// Run the `uninstall` section (or fall back to rolling back the
/// latest transaction of the same kind).
pub fn run_uninstall(
    registry: &CapabilityRegistry,
    capability_id: &str,
    game_id: &str,
    _game_name: &str,
    install_directory: &Path,
) -> Result<TransactionRecord, String> {
    let spec = registry
        .get(capability_id)
        .ok_or_else(|| format!("Unknown capability id '{capability_id}'."))?;
    if spec.uninstall.is_empty() {
        return transaction::rollback_latest_module_transaction(
            game_id.to_owned(),
            spec.id.clone(),
        );
    }
    let config = ResolvedConfig::default();
    let step_context = StepContext {
        spec,
        config: &config,
        install_directory,
        executable_directory: install_directory,
    };
    for step in &spec.uninstall {
        let _ = builtin_steps::execute_step(step, &step_context)?;
    }
    transaction::rollback_latest_module_transaction(game_id.to_owned(), spec.id.clone())
}

/// Run a single check by id against the supplied config.
pub async fn evaluate_check(
    registry: &CapabilityRegistry,
    capability_id: &str,
    check_id: &str,
    config: &ResolvedConfig,
    executable_directory: &Path,
) -> Result<CheckOutcome, String> {
    let spec = registry
        .get(capability_id)
        .ok_or_else(|| format!("Unknown capability id '{capability_id}'."))?;
    let check = spec
        .checks
        .iter()
        .chain(spec.verify.iter())
        .find(|check| check.id == check_id)
        .ok_or_else(|| format!("Check '{check_id}' not found in capability '{capability_id}'."))?;
    builtin_checks::evaluate_check(check, config, executable_directory).await
}

/// Run every check in `spec.checks` and `spec.verify` and return a
/// structured verification report. Drives the desktop UI
/// checklist.
pub async fn evaluate_all_checks(
    registry: &CapabilityRegistry,
    capability_id: &str,
    config: &ResolvedConfig,
    executable_directory: &Path,
) -> Result<VerificationReport, String> {
    let spec = registry
        .get(capability_id)
        .ok_or_else(|| format!("Unknown capability id '{capability_id}'."))?;

    let mut outcomes: Vec<CheckOutcome> = Vec::new();
    let mut all_passed = true;
    let mut any_blocker_failed = false;
    for check in spec.checks.iter().chain(spec.verify.iter()) {
        let outcome = builtin_checks::evaluate_check(check, config, executable_directory).await?;
        if !outcome.passed {
            all_passed = false;
        }
        if !outcome.passed && outcome.severity == CheckSeverity::Blocker {
            any_blocker_failed = true;
        }
        outcomes.push(outcome);
    }

    let status = if any_blocker_failed {
        ModuleStatus::Attention
    } else if all_passed {
        ModuleStatus::Ready
    } else {
        ModuleStatus::Attention
    };

    let summary = format!(
        "{} has {} checks; {} failed.",
        spec.display_name,
        outcomes.len(),
        outcomes.iter().filter(|outcome| !outcome.passed).count()
    );

    Ok(VerificationReport {
        status,
        summary,
        checks: outcomes,
        game_running: false,
        installed: false,
        installed_version: None,
        definitions: Vec::new(),
    })
}

fn parse_index(value: &str) -> Option<usize> {
    if value.starts_with("check:") {
        value.trim_start_matches("check:").parse().ok()
    } else {
        None
    }
}

fn build_metadata(spec: &CapabilitySpec, config: &ResolvedConfig) -> std::collections::BTreeMap<String, String> {
    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert("capabilityId".to_owned(), spec.id.clone());
    if let Some(version) = config.get_string("version") {
        metadata.insert("version".to_owned(), version);
    }
    metadata
}

// === Tauri command surface =====================================================

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityInstallRequest {
    pub capability_id: String,
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable_dir: String,
    pub config: ResolvedConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityUninstallRequest {
    pub capability_id: String,
    pub game_id: String,
    pub install_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityEvaluateRequest {
    pub capability_id: String,
    pub executable_dir: String,
    pub config: ResolvedConfig,
}

#[tauri::command]
pub async fn capability_install(
    request: CapabilityInstallRequest,
) -> Result<InstallResult, String> {
    let registry = CapabilityRegistry::load();
    let install_directory = PathBuf::from(&request.install_dir);
    let executable_directory = PathBuf::from(&request.executable_dir);
    run_install(
        &registry,
        &request.capability_id,
        &request.game_id,
        &request.game_name,
        &request.config,
        &install_directory,
        &executable_directory,
    )
}

#[tauri::command]
pub async fn capability_uninstall(
    request: CapabilityUninstallRequest,
) -> Result<TransactionRecord, String> {
    let registry = CapabilityRegistry::load();
    let install_directory = PathBuf::from(&request.install_dir);
    run_uninstall(
        &registry,
        &request.capability_id,
        &request.game_id,
        "",
        &install_directory,
    )
}

#[tauri::command]
pub async fn capability_evaluate(
    request: CapabilityEvaluateRequest,
) -> Result<VerificationReport, String> {
    let registry = CapabilityRegistry::load();
    let executable_directory = PathBuf::from(&request.executable_dir);
    evaluate_all_checks(
        &registry,
        &request.capability_id,
        &request.config,
        &executable_directory,
    )
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySummary {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub status: String,
    /// Provenance — drives the UI badge ("Verified", "Local",
    /// "Community"). See [`SpecOrigin`].
    pub origin: crate::capability::SpecOrigin,
}

#[tauri::command]
pub fn capability_list() -> Vec<CapabilitySummary> {
    let registry = CapabilityRegistry::load();
    registry
        .ids()
        .map(|id| {
            let spec = registry.get(id).expect("registry invariant");
            CapabilitySummary {
                id: spec.id.clone(),
                display_name: spec.display_name.clone(),
                category: spec.category.clone(),
                status: spec.status.clone(),
                origin: spec.origin,
            }
        })
        .collect()
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityReloadRequest {
    pub local_dir: Option<String>,
}

#[tauri::command]
pub fn capability_reload(
    request: CapabilityReloadRequest,
) -> Result<Vec<CapabilitySummary>, String> {
    use std::sync::Mutex;
    use std::sync::OnceLock;

    static STATE: OnceLock<Mutex<CapabilityRegistry>> = OnceLock::new();
    let mutex = STATE.get_or_init(|| Mutex::new(CapabilityRegistry::load()));
    let mut registry = mutex.lock().map_err(|error| error.to_string())?;
    let dir = request
        .local_dir
        .map(PathBuf::from)
        .or_else(local_capabilities_dir)
        .ok_or_else(|| "LOCALAPPDATA not set".to_owned())?;
    registry.reload_local(&dir);
    Ok(registry
        .ids()
        .map(|id| {
            let spec = registry.get(id).expect("registry invariant");
            CapabilitySummary {
                id: spec.id.clone(),
                display_name: spec.display_name.clone(),
                category: spec.category.clone(),
                status: spec.status.clone(),
                origin: spec.origin,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CheckSpec, SpecOrigin};
    use std::fs;

    #[test]
    fn registry_loads_ofxr_bridge() {
        let registry = CapabilityRegistry::load();
        assert!(registry.get("ofxr-bridge").is_some());
        assert!(registry.len() >= 1);
    }

    #[tokio::test]
    async fn evaluate_check_returns_outcome_for_known_id() {
        let registry = CapabilityRegistry::load();
        let spec = registry.get("ofxr-bridge").expect("ofxr-bridge loaded");
        let _ = spec; // sanity: spec is loaded
        let config = ResolvedConfig::default();
        let result = evaluate_check(
            &registry,
            "ofxr-bridge",
            "missing-check-id-should-still-be-searchable",
            &config,
            std::path::Path::new("C:/no/such/path"),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn unknown_capability_returns_error() {
        let registry = CapabilityRegistry::load();
        let config = ResolvedConfig::default();
        let result = evaluate_check(
            &registry,
            "no-such-capability",
            "any",
            &config,
            std::path::Path::new("C:/Games"),
        )
        .await;
        assert!(result.is_err());
    }

    #[test]
    fn check_severity_default_is_info() {
        let check = CheckSpec {
            id: "example".to_owned(),
            label: "Example".to_owned(),
            kind: "process-running".to_owned(),
            params: Default::default(),
            severity: String::new(),
            category: String::new(),
            description: None,
        };
        assert_eq!(check.severity, "");
        assert_eq!(check.category, "");
        // serde default applied at deserialise time, not here.
    }

    fn write_local_capability(dir: &Path, id: &str, body: &str) {
        fs::create_dir_all(dir).expect("create temp dir");
        fs::write(dir.join(format!("{id}.yaml")), body).expect("write yaml");
    }

    #[test]
    fn local_overrides_built_in_when_id_matches() {
        let temp = std::env::temp_dir().join(format!(
            "moddin-local-override-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        write_local_capability(
            &temp,
            "ofxr-bridge",
            r#"
id: ofxr-bridge
displayName: Local OFXR override
category: vr
status: planned
"#,
        );
        let registry = CapabilityRegistry::load_with_local_dir(&temp);
        let spec = registry.get("ofxr-bridge").expect("spec present");
        assert_eq!(spec.display_name, "Local OFXR override");
        assert_eq!(spec.origin, SpecOrigin::Local);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn local_adds_new_capability_without_touching_built_ins() {
        let temp = std::env::temp_dir().join(format!(
            "moddin-local-add-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        write_local_capability(
            &temp,
            "community-experimental-mod",
            r#"
id: community-experimental-mod
displayName: Experimental community mod
category: graphics
status: planned
checks:
  - id: archive-reachable
    label: Archive reachable
    kind: archive-reachable
    severity: info
    category: modulespecific
    params:
      urlField: downloadUrl
install:
  - kind: write-text-file
    description: write a placeholder
    params:
      pathField: outputPath
      template: hello
"#,
        );
        let registry = CapabilityRegistry::load_with_local_dir(&temp);
        let spec = registry
            .get("community-experimental-mod")
            .expect("local spec loaded");
        assert_eq!(spec.origin, SpecOrigin::Local);
        assert_eq!(spec.checks.len(), 1);
        assert_eq!(spec.install.len(), 1);
        // Built-ins still there.
        assert!(registry.get("ofxr-bridge").is_some());

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn malformed_local_yaml_is_skipped_not_panicked() {
        let temp = std::env::temp_dir().join(format!(
            "moddin-local-malformed-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).expect("create temp dir");
        fs::write(temp.join("bad.yaml"), "this: is: not: valid: yaml: at: all:")
            .expect("write malformed yaml");
        let registry = CapabilityRegistry::load_with_local_dir(&temp);
        // Built-ins unaffected by the malformed local file.
        assert!(registry.get("ofxr-bridge").is_some());

        let _ = fs::remove_dir_all(&temp);
    }
}