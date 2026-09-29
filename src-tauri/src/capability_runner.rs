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
    capability::{CapabilitySpec, CheckSpec, ResolvedConfig, SpecOrigin},
    module::{CheckOutcome, CheckSeverity, ModuleStatus, VerificationReport},
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
    /// Ids of capabilities auto-installed as dependencies of this
    /// install, in install order (a dependency always precedes the
    /// capability that required it).
    #[serde(default)]
    pub installed_dependencies: Vec<String>,
    /// Evaluated `exe-version` outcome when the spec declares a
    /// `compatibility` block, `None` when the spec supports every
    /// game build. Even on a forced install the evaluated outcome is
    /// reported here so the UI can surface it.
    #[serde(default)]
    pub compatibility: Option<CheckOutcome>,
}

/// Maximum number of capability ids a dependency chain may contain
/// before the runner refuses to recurse further (guards both runaway
/// recipes and cycles the id-guard misses).
pub const MAX_DEPENDENCY_DEPTH: usize = 8;

/// Stable id of the synthesized compatibility check the runner adds
/// to verification reports (and to `InstallResult.compatibility`) for
/// specs that declare a `compatibility` block.
pub const COMPATIBILITY_CHECK_ID: &str = "exe-version-compat";

/// Internal install worker. Called by `run_install` (registry lookup)
/// and `run_install_with_spec` (community YAML the caller already
/// fetched + verified). Same transaction store, same step dispatch.
fn execute_install(
    spec: &CapabilitySpec,
    game_id: &str,
    game_name: &str,
    config: &ResolvedConfig,
    install_directory: &Path,
    executable_directory: &Path,
) -> Result<InstallResult, String> {
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
        installed_dependencies: Vec::new(),
        compatibility: None,
    })
}

/// A capability counts as installed for a game when the transaction
/// store holds an applied record for the pair — the exact predicate
/// the uninstall path uses before rolling back.
fn is_capability_installed(game_id: &str, capability_id: &str) -> bool {
    transaction::list_transactions_sync()
        .map(|records| {
            records.iter().any(|record| {
                record.status == "applied"
                    && record.game_id == game_id
                    && record.kind == capability_id
            })
        })
        .unwrap_or(false)
}

/// Recursively install every missing dependency declared by `spec`.
/// Missing = no applied transaction for (gameId, dependencyId).
/// Transitive dependencies install first; `chain` holds the ids from
/// the originally requested capability down to (and including)
/// `spec`, which powers both the cycle guard and the error messages.
fn install_dependencies_recursive(
    registry: &CapabilityRegistry,
    spec: &CapabilitySpec,
    game_id: &str,
    game_name: &str,
    config: &ResolvedConfig,
    install_directory: &Path,
    executable_directory: &Path,
    chain: &mut Vec<String>,
    installed_dependencies: &mut Vec<String>,
) -> Result<(), String> {
    for dependency_id in &spec.dependencies {
        if is_capability_installed(game_id, dependency_id) {
            continue;
        }

        let mut chain_text = chain.clone();
        chain_text.push(dependency_id.clone());
        let chain_label = chain_text.join(" -> ");
        if chain.len() + 1 > MAX_DEPENDENCY_DEPTH {
            return Err(format!(
                "Dependency chain too deep (max {MAX_DEPENDENCY_DEPTH} levels): {chain_label}. \
                 Flatten the recipe or split the chain before installing."
            ));
        }
        if chain.iter().any(|seen| seen == dependency_id) {
            return Err(format!(
                "Dependency cycle detected: {chain_label}. \
                 Remove the cycle from the capability recipes before installing."
            ));
        }
        let dependency_spec = registry.get(dependency_id).ok_or_else(|| {
            format!(
                "Missing dependency '{dependency_id}' required by '{}' (chain: {chain_label}). \
                 It is not a loaded capability; install it manually first.",
                spec.id
            )
        })?;

        chain.push(dependency_id.clone());
        let outcome = (|| -> Result<(), String> {
            install_dependencies_recursive(
                registry,
                dependency_spec,
                game_id,
                game_name,
                config,
                install_directory,
                executable_directory,
                chain,
                installed_dependencies,
            )?;
            // The gate also applies to auto-installed dependencies,
            // but `force` never cascades: overriding an incompatible
            // game build is a per-capability, user-level decision.
            if let Some(compatibility) = evaluate_compatibility(
                dependency_spec,
                executable_directory,
                &mut builtin_checks::ExeVersionCache::new(),
            ) {
                if !compatibility.passed {
                    return Err(format!(
                        "Dependency '{}' (required by '{}') is incompatible with this game \
                         build: {}. Install it on its own — with force=true — only if you \
                         accept the risk.",
                        dependency_spec.id,
                        spec.id,
                        compatibility
                            .detail
                            .unwrap_or_else(|| "game executable outside the supported version range".to_owned())
                    ));
                }
            }
            execute_install(
                dependency_spec,
                game_id,
                game_name,
                config,
                install_directory,
                executable_directory,
            )?;
            installed_dependencies.push(dependency_id.clone());
            Ok(())
        })();
        chain.pop();
        outcome?;
    }
    Ok(())
}

/// Build and evaluate the synthesized `exe-version` check for a
/// spec's `compatibility` block. Returns `None` when the spec does
/// not constrain the game build (no block, or a block without any
/// bound), which must behave exactly like "compatible with
/// everything".
///
/// A blank `executable_directory` also returns `None`: the caller has
/// no game to check against (the AI recommend flow and the community
/// panel install without a selected game, so they pass an empty
/// string), and failing closed there would block every community spec
/// that declares a compatibility block. An *existing* executable whose
/// version cannot be read is a different case and still fails — that
/// one is a real "I could not verify this build".
fn evaluate_compatibility(
    spec: &CapabilitySpec,
    executable_directory: &Path,
    exe_versions: &mut builtin_checks::ExeVersionCache,
) -> Option<CheckOutcome> {
    let compatibility = spec.compatibility.as_ref()?;
    if !compatibility.has_constraints() {
        return None;
    }
    if executable_directory.as_os_str().is_empty() {
        return None;
    }
    let mut params: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    params.insert(
        "path".to_owned(),
        serde_json::Value::String(compatibility.game_exe.clone().unwrap_or_default()),
    );
    if let Some(min) = &compatibility.min_exe_version {
        params.insert("minVersion".to_owned(), serde_json::Value::String(min.clone()));
    }
    if let Some(max) = &compatibility.max_exe_version {
        params.insert("maxVersion".to_owned(), serde_json::Value::String(max.clone()));
    }
    if !compatibility.blocked_exe_versions.is_empty() {
        params.insert(
            "blockedVersions".to_owned(),
            serde_json::Value::Array(
                compatibility
                    .blocked_exe_versions
                    .iter()
                    .map(|version| serde_json::Value::String(version.clone()))
                    .collect(),
            ),
        );
    }
    let check = CheckSpec {
        id: COMPATIBILITY_CHECK_ID.to_owned(),
        label: format!("{} supports this game build", spec.display_name),
        kind: "exe-version".to_owned(),
        params,
        severity: crate::capability::severity::BLOCKER.to_owned(),
        category: "modulespecific".to_owned(),
        description: Some(
            "Synthesized from the capability compatibility block; fails when the game \
             executable's FileVersion is outside the supported range."
                .to_owned(),
        ),
    };
    Some(builtin_checks::evaluate_exe_version(
        &check,
        &ResolvedConfig::default(),
        executable_directory,
        exe_versions,
    ))
}

/// Shared install path for registry-resolved and caller-supplied
/// specs: compatibility gate → dependency auto-install → `install`
/// steps. `force` bypasses the compatibility gate for this spec only.
fn install_spec(
    registry: &CapabilityRegistry,
    spec: &CapabilitySpec,
    game_id: &str,
    game_name: &str,
    config: &ResolvedConfig,
    install_directory: &Path,
    executable_directory: &Path,
    force: bool,
) -> Result<InstallResult, String> {
    let compatibility = evaluate_compatibility(
        spec,
        executable_directory,
        &mut builtin_checks::ExeVersionCache::new(),
    );
    if let Some(outcome) = &compatibility {
        if !outcome.passed && !force {
            return Err(format!(
                "'{}' is not compatible with this game build: {}. Re-run with force=true \
                 to install anyway.",
                spec.display_name,
                outcome.detail.clone().unwrap_or_else(|| {
                    "game executable outside the supported version range".to_owned()
                })
            ));
        }
    }

    let mut installed_dependencies = Vec::new();
    let mut chain = vec![spec.id.clone()];
    install_dependencies_recursive(
        registry,
        spec,
        game_id,
        game_name,
        config,
        install_directory,
        executable_directory,
        &mut chain,
        &mut installed_dependencies,
    )?;

    let mut result = execute_install(
        spec,
        game_id,
        game_name,
        config,
        install_directory,
        executable_directory,
    )?;
    result.installed_dependencies = installed_dependencies;
    result.compatibility = compatibility;
    Ok(result)
}

/// Run the `install` section of a capability that lives in the
/// registry. Missing `dependencies` are auto-installed first (up to
/// [`MAX_DEPENDENCY_DEPTH`] levels); `force` bypasses the spec's
/// compatibility gate for this capability only.
pub fn run_install(
    registry: &CapabilityRegistry,
    capability_id: &str,
    game_id: &str,
    game_name: &str,
    config: &ResolvedConfig,
    install_directory: &Path,
    executable_directory: &Path,
    force: bool,
) -> Result<InstallResult, String> {
    let spec = registry
        .get(capability_id)
        .ok_or_else(|| format!("Unknown capability id '{capability_id}'."))?;
    install_spec(
        registry,
        spec,
        game_id,
        game_name,
        config,
        install_directory,
        executable_directory,
        force,
    )
}

/// Run the `install` section of a capability whose `CapabilitySpec`
/// was supplied directly (no registry lookup). Used by
/// `community_capability_install` after the caller has downloaded
/// and signature-verified the YAML. Dependencies resolve against the
/// regular registry (built-ins + local overrides).
pub fn run_install_with_spec(
    registry: &CapabilityRegistry,
    spec: &CapabilitySpec,
    game_id: &str,
    game_name: &str,
    config: &ResolvedConfig,
    install_directory: &Path,
    executable_directory: &Path,
    force: bool,
) -> Result<InstallResult, String> {
    install_spec(
        registry,
        spec,
        game_id,
        game_name,
        config,
        install_directory,
        executable_directory,
        force,
    )
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
    builtin_checks::evaluate_check(
        check,
        config,
        executable_directory,
        &mut builtin_checks::ExeVersionCache::new(),
    )
    .await
}

/// Run every check in `spec.checks` and `spec.verify` and return a
/// structured verification report. Drives the desktop UI
/// checklist. When the spec declares a `compatibility` block, the
/// synthesized `exe-version` outcome is reported first under the
/// stable id [`COMPATIBILITY_CHECK_ID`].
pub async fn evaluate_all_checks(
    registry: &CapabilityRegistry,
    capability_id: &str,
    config: &ResolvedConfig,
    executable_directory: &Path,
) -> Result<VerificationReport, String> {
    let spec = registry
        .get(capability_id)
        .ok_or_else(|| format!("Unknown capability id '{capability_id}'."))?;

    let mut exe_versions = builtin_checks::ExeVersionCache::new();
    let mut outcomes: Vec<CheckOutcome> = Vec::new();
    if let Some(compatibility) =
        evaluate_compatibility(spec, executable_directory, &mut exe_versions)
    {
        outcomes.push(compatibility);
    }
    let mut all_passed = outcomes.iter().all(|outcome| outcome.passed);
    let mut any_blocker_failed = outcomes
        .iter()
        .any(|outcome| !outcome.passed && outcome.severity == CheckSeverity::Blocker);
    for check in spec.checks.iter().chain(spec.verify.iter()) {
        let outcome =
            builtin_checks::evaluate_check(check, config, executable_directory, &mut exe_versions)
                .await?;
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

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use serde_yaml;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityInstallRequest {
    pub capability_id: String,
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable_dir: String,
    pub config: ResolvedConfig,
    /// Install even when the spec's `compatibility` block reports the
    /// game build as unsupported. The evaluated outcome is still
    /// returned in `InstallResult.compatibility` so the UI can show
    /// what the user overrode. Defaults to `false`.
    #[serde(default)]
    pub force: bool,
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
        request.force,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCompatibilityRequest {
    pub capability_id: String,
    pub executable_dir: String,
}

/// Evaluate ONLY the spec's `compatibility` block.
///
/// `capability_evaluate` is the full preflight, but it also runs every
/// declared check — including `archive-sha256`, which downloads the
/// archive. That is far too slow to run while rendering module cards,
/// so the UI calls this instead: once per spec that declares a
/// compatibility block, to explain the block and offer a forced
/// install before the user ever clicks Apply.
///
/// `Ok(None)` means the spec does not constrain the game build (no
/// block, or a block without a bound) and is compatible with
/// everything — the UI shows no warning at all in that case.
#[tauri::command]
pub fn capability_compatibility(
    request: CapabilityCompatibilityRequest,
) -> Result<Option<CheckOutcome>, String> {
    let registry = CapabilityRegistry::load();
    let spec = registry
        .get(&request.capability_id)
        .ok_or_else(|| format!("Unknown capability id '{}'.", request.capability_id))?;
    Ok(evaluate_compatibility(
        spec,
        &PathBuf::from(&request.executable_dir),
        &mut builtin_checks::ExeVersionCache::new(),
    ))
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityGetRequest {
    pub capability_id: String,
}

/// Return the full spec for one capability. `capability_list` only
/// surfaces summaries; the game-detail module cards need the complete
/// recipe (`configSchema`, `safetyNotes`, `supportedEngines`, checks)
/// to render their typed config form.
#[tauri::command]
pub fn capability_get(request: CapabilityGetRequest) -> Result<CapabilitySpec, String> {
    let registry = CapabilityRegistry::load();
    registry
        .get(&request.capability_id)
        .cloned()
        .ok_or_else(|| format!("Unknown capability id '{}'.", request.capability_id))
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

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignedBy {
    algorithm: String,
    public_key: String,
    signature: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct SignedByFile {
    signed_by: SignedBy,
}

fn verify_signed_by(yaml_text: &str, signed_by_text: &str) -> Result<(), String> {
    let payload = yaml_text.as_bytes();
    let parsed: SignedByFile = serde_yaml::from_str(signed_by_text)
        .map_err(|error| format!("SIGNED-BY is not valid YAML: {error}"))?;
    let signed_by = &parsed.signed_by;
    if signed_by.algorithm != "ed25519" {
        return Err(format!(
            "SIGNED-BY uses unsupported algorithm '{}' (expected 'ed25519').",
            signed_by.algorithm
        ));
    }
    let public_bytes = BASE64
        .decode(signed_by.public_key.as_bytes())
        .map_err(|error| format!("SIGNED-BY publicKey is not valid base64: {error}"))?;
    if public_bytes.len() != 32 {
        return Err(format!(
            "SIGNED-BY publicKey must be 32 bytes (got {}).",
            public_bytes.len()
        ));
    }
    let public_array: [u8; 32] = public_bytes
        .as_slice()
        .try_into()
        .map_err(|error: std::array::TryFromSliceError| error.to_string())?;
    let public_key = VerifyingKey::from_bytes(&public_array)
        .map_err(|error| format!("SIGNED-BY publicKey is not a valid Ed25519 key: {error}"))?;
    let signature_bytes = BASE64
        .decode(signed_by.signature.as_bytes())
        .map_err(|error| format!("SIGNED-BY signature is not valid base64: {error}"))?;
    if signature_bytes.len() != 64 {
        return Err(format!(
            "SIGNED-BY signature must be 64 bytes (got {}).",
            signature_bytes.len()
        ));
    }
    let signature_array: [u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|error: std::array::TryFromSliceError| error.to_string())?;
    let signature = Signature::from_bytes(&signature_array);
    public_key
        .verify(payload, &signature)
        .map_err(|error| format!("SIGNED-BY signature verification failed: {error}"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityInstallRequest {
    pub capability_id: String,
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable_dir: String,
    pub config: ResolvedConfig,
    /// When false, an unsigned community capability aborts with an
    /// error. When true, the caller has already confirmed the install
    /// in the UI and the runner accepts unsigned capabilities.
    #[serde(default)]
    pub accept_unsigned: bool,
    /// Same override as `CapabilityInstallRequest.force`: installs
    /// even when the spec's `compatibility` block rejects the game
    /// build. Defaults to `false`.
    #[serde(default)]
    pub force: bool,
}

#[tauri::command]
pub async fn community_capability_install(
    request: CommunityInstallRequest,
) -> Result<InstallResult, String> {
    let catalog = crate::community_catalog::read_cached_catalog_only()
        .ok_or_else(|| {
            "Community catalog has not been fetched yet. Call community_catalog_fetch first."
                .to_owned()
        })?;
    let entry = catalog
        .capabilities
        .iter()
        .find(|candidate| candidate.id == request.capability_id)
        .ok_or_else(|| {
            format!(
                "Community catalog does not list a capability named '{}'.",
                request.capability_id
            )
        })?;
    let download_url = entry.download_url.clone().ok_or_else(|| {
        format!(
            "Community catalog entry '{}' has no downloadUrl.",
            request.capability_id
        )
    })?;

    let (yaml_text, signed_by_text) =
        crate::community_catalog::fetch_capability_yaml(&download_url).await?;

    if let Some(signed_by) = signed_by_text.as_deref() {
        if let Err(error) = verify_signed_by(&yaml_text, signed_by) {
            return Err(format!(
                "Community capability '{}' signature invalid: {error}",
                request.capability_id
            ));
        }
    } else if !request.accept_unsigned {
        return Err(format!(
            "Community capability '{}' is not signed. Re-run with acceptUnsigned=true after the UI confirmation.",
            request.capability_id
        ));
    }

    let spec: CapabilitySpec = serde_yaml::from_str(&yaml_text)
        .map_err(|error| format!("could not parse community capability YAML: {error}"))?;
    if spec.id != request.capability_id {
        return Err(format!(
            "Community capability id mismatch: catalog says '{}' but the downloaded YAML says '{}'.",
            request.capability_id,
            spec.id
        ));
    }

    let install_directory = PathBuf::from(&request.install_dir);
    let executable_directory = PathBuf::from(&request.executable_dir);
    let registry = CapabilityRegistry::load();
    run_install_with_spec(
        &registry,
        &spec,
        &request.game_id,
        &request.game_name,
        &request.config,
        &install_directory,
        &executable_directory,
        request.force,
    )
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

    #[test]
    fn capability_get_returns_spec_for_known_id() {
        let result = capability_get(CapabilityGetRequest {
            capability_id: "ofxr-bridge".to_owned(),
        });
        let spec = result.expect("ofxr-bridge is a built-in capability");
        assert_eq!(spec.id, "ofxr-bridge");
        assert_eq!(spec.origin, SpecOrigin::BuiltIn);
    }

    #[test]
    fn capability_get_errors_for_unknown_id() {
        let result = capability_get(CapabilityGetRequest {
            capability_id: "no-such-capability".to_owned(),
        });
        assert!(result.is_err());
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

    // === Dependency graph + compatibility gate ======================

    fn temp_root(suffix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "moddin-cap-runner-{}-{}-{}",
            suffix,
            std::process::id(),
            nanos
        ))
    }

    /// Point LOCALAPPDATA at a private dir so the transaction store is
    /// hermetic, and hold the shared env lock for the whole test body
    /// (same protocol as compat_report's env tests).
    struct IsolatedAppdata {
        dir: PathBuf,
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl IsolatedAppdata {
        fn new(suffix: &str) -> Self {
            let lock = crate::test_support::env_lock();
            let dir = temp_root(suffix);
            std::fs::create_dir_all(&dir).expect("isolated appdata dir");
            let previous = std::env::var_os("LOCALAPPDATA");
            std::env::set_var("LOCALAPPDATA", &dir);
            Self {
                dir,
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for IsolatedAppdata {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var("LOCALAPPDATA", value),
                None => std::env::remove_var("LOCALAPPDATA"),
            }
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn marker_step(field: &str) -> String {
        format!(
            "install:\n  - kind: write-text-file\n    description: write marker\n    params:\n      pathField: {field}\n      template: marker\n"
        )
    }

    #[test]
    fn installs_missing_dependencies_before_the_dependent() {
        let _appdata = IsolatedAppdata::new("dep-order");
        let caps = temp_root("dep-order-caps");
        write_local_capability(
            &caps,
            "dep-base",
            &format!(
                "id: dep-base\ndisplayName: Dep base\ncategory: system\nstatus: available\n{}",
                marker_step("markerBase")
            ),
        );
        write_local_capability(
            &caps,
            "dep-middle",
            &format!(
                "id: dep-middle\ndisplayName: Dep middle\ncategory: system\nstatus: available\ndependencies:\n  - dep-base\n{}",
                marker_step("markerMiddle")
            ),
        );
        write_local_capability(
            &caps,
            "dep-top",
            &format!(
                "id: dep-top\ndisplayName: Dep top\ncategory: system\nstatus: available\ndependencies:\n  - dep-middle\n{}",
                marker_step("markerTop")
            ),
        );

        let work = temp_root("dep-order-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        let mut config = ResolvedConfig::default();
        config.values.insert(
            "markerBase".to_owned(),
            serde_json::Value::String(executable.join("base.txt").to_string_lossy().into_owned()),
        );
        config.values.insert(
            "markerMiddle".to_owned(),
            serde_json::Value::String(
                executable.join("middle.txt").to_string_lossy().into_owned(),
            ),
        );
        config.values.insert(
            "markerTop".to_owned(),
            serde_json::Value::String(executable.join("top.txt").to_string_lossy().into_owned()),
        );

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let game_id = "dep-order-game";
        let result = run_install(
            &registry,
            "dep-top",
            game_id,
            "Dep Order Game",
            &config,
            &work,
            &executable,
            false,
        )
        .expect("install with auto-installed dependencies");

        // Transitive dependency installs first, then its dependent.
        assert_eq!(result.installed_dependencies, vec!["dep-base", "dep-middle"]);
        assert!(result.compatibility.is_none());
        for name in ["base.txt", "middle.txt", "top.txt"] {
            assert!(
                executable.join(name).is_file(),
                "{name} was written by the install chain"
            );
        }
        // Every capability in the chain recorded an applied transaction.
        for capability_id in ["dep-base", "dep-middle", "dep-top"] {
            assert!(
                is_capability_installed(game_id, capability_id),
                "{capability_id} is active for the game"
            );
        }

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&caps);
    }

    #[test]
    fn already_installed_dependency_is_skipped() {
        let _appdata = IsolatedAppdata::new("dep-skip");
        let caps = temp_root("dep-skip-caps");
        write_local_capability(
            &caps,
            "dep-present",
            &format!(
                "id: dep-present\ndisplayName: Dep present\ncategory: system\nstatus: available\n{}",
                marker_step("markerPresent")
            ),
        );
        write_local_capability(
            &caps,
            "dep-requester",
            &format!(
                "id: dep-requester\ndisplayName: Dep requester\ncategory: system\nstatus: available\ndependencies:\n  - dep-present\n{}",
                marker_step("markerRequester")
            ),
        );

        let work = temp_root("dep-skip-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        let mut config = ResolvedConfig::default();
        config.values.insert(
            "markerPresent".to_owned(),
            serde_json::Value::String(
                executable.join("present.txt").to_string_lossy().into_owned(),
            ),
        );
        config.values.insert(
            "markerRequester".to_owned(),
            serde_json::Value::String(
                executable.join("requester.txt").to_string_lossy().into_owned(),
            ),
        );

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let game_id = "dep-skip-game";
        run_install(
            &registry,
            "dep-present",
            game_id,
            "Dep Skip Game",
            &config,
            &work,
            &executable,
            false,
        )
        .expect("install the dependency on its own");

        let result = run_install(
            &registry,
            "dep-requester",
            game_id,
            "Dep Skip Game",
            &config,
            &work,
            &executable,
            false,
        )
        .expect("install the requester");
        assert!(
            result.installed_dependencies.is_empty(),
            "an already-active dependency must not be reinstalled"
        );

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&caps);
    }

    #[test]
    fn dependency_cycle_is_detected_with_the_chain() {
        let _appdata = IsolatedAppdata::new("dep-cycle");
        let caps = temp_root("dep-cycle-caps");
        write_local_capability(
            &caps,
            "cycle-alpha",
            "id: cycle-alpha\ndisplayName: Cycle alpha\ncategory: system\nstatus: available\ndependencies:\n  - cycle-beta\n",
        );
        write_local_capability(
            &caps,
            "cycle-beta",
            "id: cycle-beta\ndisplayName: Cycle beta\ncategory: system\nstatus: available\ndependencies:\n  - cycle-alpha\n",
        );
        write_local_capability(
            &caps,
            "cycle-self",
            "id: cycle-self\ndisplayName: Cycle self\ncategory: system\nstatus: available\ndependencies:\n  - cycle-self\n",
        );

        let work = temp_root("dep-cycle-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let config = ResolvedConfig::default();

        let error = run_install(
            &registry,
            "cycle-alpha",
            "cycle-game",
            "Cycle Game",
            &config,
            &work,
            &executable,
            false,
        )
        .expect_err("cycle aborts the install");
        assert!(error.contains("cycle"), "error names the problem: {error}");
        assert!(
            error.contains("cycle-alpha -> cycle-beta -> cycle-alpha"),
            "error lists the full chain: {error}"
        );

        let error = run_install(
            &registry,
            "cycle-self",
            "cycle-game",
            "Cycle Game",
            &config,
            &work,
            &executable,
            false,
        )
        .expect_err("self-dependency aborts the install");
        assert!(
            error.contains("cycle-self -> cycle-self"),
            "self-dependency lists the chain: {error}"
        );

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&caps);
    }

    #[test]
    fn dependency_chain_depth_is_capped() {
        let _appdata = IsolatedAppdata::new("dep-depth");
        let caps = temp_root("dep-depth-caps");
        for index in 0..9 {
            write_local_capability(
                &caps,
                &format!("chain-{index}"),
                &format!(
                    "id: chain-{index}\ndisplayName: Chain {index}\ncategory: system\nstatus: available\ndependencies:\n  - chain-{}\n",
                    index + 1
                ),
            );
        }

        let work = temp_root("dep-depth-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        let registry = CapabilityRegistry::load_with_local_dir(&caps);

        let error = run_install(
            &registry,
            "chain-0",
            "depth-game",
            "Depth Game",
            &ResolvedConfig::default(),
            &work,
            &executable,
            false,
        )
        .expect_err("runaway chain aborts the install");
        assert!(
            error.contains("too deep"),
            "error names the depth cap: {error}"
        );
        assert!(
            error.contains(&format!("chain-{}", MAX_DEPENDENCY_DEPTH)),
            "error lists the chain up to the cap: {error}"
        );

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&caps);
    }

    #[test]
    fn unknown_dependency_reports_the_missing_chain() {
        let _appdata = IsolatedAppdata::new("dep-missing");
        let caps = temp_root("dep-missing-caps");
        write_local_capability(
            &caps,
            "dep-orphan",
            "id: dep-orphan\ndisplayName: Dep orphan\ncategory: system\nstatus: available\ndependencies:\n  - ghost-loader\n",
        );

        let work = temp_root("dep-missing-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        let registry = CapabilityRegistry::load_with_local_dir(&caps);

        let error = run_install(
            &registry,
            "dep-orphan",
            "missing-game",
            "Missing Game",
            &ResolvedConfig::default(),
            &work,
            &executable,
            false,
        )
        .expect_err("unknown dependency aborts the install");
        assert!(
            error.contains("ghost-loader"),
            "error names the missing dependency: {error}"
        );
        assert!(
            error.contains("dep-orphan -> ghost-loader"),
            "error lists the dependency chain: {error}"
        );

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&caps);
    }

    /// Writes a local capability whose compatibility block requires
    /// game.exe >= 99.0.0 — every real probe of the dummy exe below is
    /// guaranteed to fail the gate (no version resource / undetectable).
    fn write_gated_capability(caps: &Path, marker_field: &str) {
        write_local_capability(
            caps,
            "gated-mod",
            &format!(
                "id: gated-mod\ndisplayName: Gated mod\ncategory: system\nstatus: available\ncompatibility:\n  gameExe: game.exe\n  minExeVersion: 99.0.0.0\n{}",
                marker_step(marker_field)
            ),
        );
    }

    #[test]
    fn incompatible_game_build_blocks_install_without_force() {
        let _appdata = IsolatedAppdata::new("gate-block");
        let caps = temp_root("gate-block-caps");
        write_gated_capability(&caps, "markerGated");

        let work = temp_root("gate-block-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        // Dummy exe: real PowerShell probe finds no FileVersion, so the
        // gate fails without needing a versioned PE binary.
        fs::write(executable.join("game.exe"), b"not a real binary").expect("dummy exe");

        let mut config = ResolvedConfig::default();
        config.values.insert(
            "markerGated".to_owned(),
            serde_json::Value::String(
                executable.join("gated.txt").to_string_lossy().into_owned(),
            ),
        );

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let error = run_install(
            &registry,
            "gated-mod",
            "gate-game",
            "Gate Game",
            &config,
            &work,
            &executable,
            false,
        )
        .expect_err("incompatible build blocks a plain install");
        assert!(
            error.contains("not compatible with this game build"),
            "error explains the incompatibility: {error}"
        );
        assert!(
            error.contains("force=true"),
            "error tells the caller how to override: {error}"
        );
        assert!(!executable.join("gated.txt").exists());
        assert!(
            !is_capability_installed("gate-game", "gated-mod"),
            "a blocked install must not record a transaction"
        );

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&caps);
    }

    #[test]
    fn force_installs_despite_incompatible_build_and_reports_outcome() {
        let _appdata = IsolatedAppdata::new("gate-force");
        let caps = temp_root("gate-force-caps");
        write_gated_capability(&caps, "markerGated");

        let work = temp_root("gate-force-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        fs::write(executable.join("game.exe"), b"not a real binary").expect("dummy exe");

        let mut config = ResolvedConfig::default();
        config.values.insert(
            "markerGated".to_owned(),
            serde_json::Value::String(
                executable.join("gated.txt").to_string_lossy().into_owned(),
            ),
        );

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let result = run_install(
            &registry,
            "gated-mod",
            "gate-game",
            "Gate Game",
            &config,
            &work,
            &executable,
            true,
        )
        .expect("force bypasses the compatibility gate");
        assert!(executable.join("gated.txt").is_file());
        let compatibility = result
            .compatibility
            .expect("the evaluated compatibility outcome is reported");
        assert_eq!(compatibility.id.as_deref(), Some(COMPATIBILITY_CHECK_ID));
        assert!(
            !compatibility.passed,
            "the outcome still records that the build is unsupported"
        );

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&caps);
    }

    #[tokio::test]
    async fn evaluate_reports_compatibility_as_a_structured_check() {
        let _appdata = IsolatedAppdata::new("gate-eval");
        let caps = temp_root("gate-eval-caps");
        write_gated_capability(&caps, "markerGated");

        let work = temp_root("gate-eval-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        fs::write(executable.join("game.exe"), b"not a real binary").expect("dummy exe");

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let report = evaluate_all_checks(
            &registry,
            "gated-mod",
            &ResolvedConfig::default(),
            &executable,
        )
        .await
        .expect("evaluation succeeds even for an incompatible build");

        let first = report.checks.first().expect("compatibility row exists");
        assert_eq!(first.id.as_deref(), Some(COMPATIBILITY_CHECK_ID));
        assert_eq!(first.severity, CheckSeverity::Blocker);
        assert!(
            !first.passed,
            "the dummy exe reports no FileVersion, so the row fails"
        );
        assert!(
            first.detail.as_deref().unwrap_or_default().contains("game.exe"),
            "detail names the probed exe: {:?}",
            first.detail
        );

        let _ = fs::remove_dir_all(&work);
        let _ = fs::remove_dir_all(&caps);
    }

    #[test]
    fn compatibility_command_reports_the_gate_without_the_checklist() {
        let appdata = IsolatedAppdata::new("compat-cmd");
        // The command loads the registry the way the app does, so the
        // recipes have to sit under the isolated LOCALAPPDATA.
        let caps = appdata.dir.join("Moddin").join("capabilities");
        write_gated_capability(&caps, "markerGated");
        write_local_capability(
            &caps,
            "ungated-mod",
            &format!(
                "id: ungated-mod\ndisplayName: Ungated mod\ncategory: system\nstatus: available\n{}",
                marker_step("markerUngated")
            ),
        );

        let work = temp_root("compat-cmd-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        fs::write(executable.join("game.exe"), b"not a real binary").expect("dummy exe");
        let executable_dir = executable.to_string_lossy().into_owned();

        let gated = capability_compatibility(CapabilityCompatibilityRequest {
            capability_id: "gated-mod".to_owned(),
            executable_dir: executable_dir.clone(),
        })
        .expect("a known id resolves")
        .expect("a block with a bound yields an outcome");
        assert_eq!(gated.id.as_deref(), Some(COMPATIBILITY_CHECK_ID));
        assert_eq!(gated.severity, CheckSeverity::Blocker);
        assert!(
            !gated.passed,
            "the dummy exe exposes no FileVersion, so the gate fails"
        );

        let ungated = capability_compatibility(CapabilityCompatibilityRequest {
            capability_id: "ungated-mod".to_owned(),
            executable_dir: executable_dir.clone(),
        })
        .expect("a known id resolves");
        assert!(
            ungated.is_none(),
            "no compatibility block means compatible with every build, \
             and the UI must show no warning"
        );

        let error = capability_compatibility(CapabilityCompatibilityRequest {
            capability_id: "does-not-exist".to_owned(),
            executable_dir: executable_dir,
        })
        .expect_err("an unknown id is an error, not a silent None");
        assert!(
            error.contains("Unknown capability id"),
            "the error names the id: {error}"
        );

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn compatibility_is_skipped_when_the_caller_has_no_game_directory() {
        let appdata = IsolatedAppdata::new("compat-nodir");
        let caps = appdata.dir.join("Moddin").join("capabilities");
        write_gated_capability(&caps, "markerGated");

        let work = temp_root("compat-nodir-work");
        let registry = CapabilityRegistry::load_with_local_dir(&caps);

        // The AI recommend flow and the community panel install without a
        // selected game, so they send an empty executable dir. There is no
        // build to verify there, and failing closed would make every
        // community spec with a compatibility block uninstallable.
        let outcome = evaluate_compatibility(
            registry.get("gated-mod").expect("gated spec loads"),
            Path::new(""),
            &mut builtin_checks::ExeVersionCache::new(),
        );
        assert!(
            outcome.is_none(),
            "no game directory means no build constraint, not a failed one"
        );

        // A directory that exists but holds no readable exe is a real
        // "could not verify" and must still fail closed.
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        fs::write(executable.join("game.exe"), b"not a real binary").expect("dummy exe");
        let outcome = evaluate_compatibility(
            registry.get("gated-mod").expect("gated spec loads"),
            &executable,
            &mut builtin_checks::ExeVersionCache::new(),
        )
        .expect("a real directory still evaluates");
        assert!(
            !outcome.passed,
            "an unreadable FileVersion is a failed verification, not an absent one"
        );

        let _ = fs::remove_dir_all(&work);
    }
}