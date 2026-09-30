//! Capability registry and runtime orchestration.
//!
//! Loads every YAML capability recipe under `src-tauri/capabilities/`
//! at build time (via `include_str!`) and exposes a small dispatcher
//! that the Tauri command layer (and the future in-process modules)
//! can call to:
//!
//! * `run_install` — execute every step in `spec.install`,
//!   record a transaction, and return the structured result.
//! * `run_uninstall` — run the uninstall steps (or rely on the
//!   transaction store to roll back the install record).
//! * `evaluate_check` — run a single check by id against the
//!   current game state (test-only; the command layer uses
//!   `evaluate_all_checks`).
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
use std::path::PathBuf;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

const OFXR_BRIDGE_YAML: &str = include_str!("../capabilities/ofxr-bridge.yaml");
const OPTISCALER_YAML: &str = include_str!("../capabilities/optiscaler.yaml");
const CHEEKY_FOVEATED_DLSS_YAML: &str = include_str!("../capabilities/cheeky-foveated-dlss.yaml");
const RESHADE_YAML: &str = include_str!("../capabilities/reshade.yaml");
const OPENXR_HELPERS_YAML: &str = include_str!("../capabilities/openxr-helpers.yaml");
const UEVR_YAML: &str = include_str!("../capabilities/uevr.yaml");
const BEPINEX_YAML: &str = include_str!("../capabilities/bepinex.yaml");
const UE4SS_YAML: &str = include_str!("../capabilities/ue4ss.yaml");
const REFRAMEWORK_YAML: &str = include_str!("../capabilities/reframework.yaml");

/// Every capability recipe compiled into the binary.
///
/// Kept as one list so [`CapabilityRegistry::load`] and the test-only
/// `load_with_local_dir` cannot drift apart, and so
/// `tests::every_built_in_spec_installs_from_a_local_archive` can cover
/// each recipe automatically as specs are added.
const BUILT_IN_YAML: &[&str] = &[
    OFXR_BRIDGE_YAML,
    OPTISCALER_YAML,
    CHEEKY_FOVEATED_DLSS_YAML,
    RESHADE_YAML,
    OPENXR_HELPERS_YAML,
    UEVR_YAML,
    BEPINEX_YAML,
    UE4SS_YAML,
    REFRAMEWORK_YAML,
];

/// Parse the built-in recipe list into `specs`, failing loudly on a
/// duplicate id or malformed YAML.
fn insert_built_ins(specs: &mut HashMap<String, CapabilitySpec>) {
    for raw in BUILT_IN_YAML {
        match serde_yaml::from_str::<CapabilitySpec>(raw) {
            Ok(mut spec) => {
                spec.origin = SpecOrigin::BuiltIn;
                if specs.contains_key(&spec.id) {
                    panic!("duplicate built-in capability id '{id}'", id = spec.id);
                }
                specs.insert(spec.id.clone(), spec);
            }
            Err(error) => {
                // A YAML scalar containing ": " silently becomes a
                // mapping, which is the single most common authoring
                // mistake in a safety note or a description. Name the
                // recipe so the failure is actionable.
                let id = raw
                    .lines()
                    .find_map(|line| line.strip_prefix("id:"))
                    .map(str::trim)
                    .unwrap_or("<no id>");
                panic!("could not parse built-in capability '{id}': {error}");
            }
        }
    }
}

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
        insert_built_ins(&mut specs);

        if let Some(local_dir) = local_capabilities_dir() {
            Self::load_local_into(&local_dir, &mut specs);
        }

        Self { specs }
    }

    /// Same as [`Self::load`] but takes the local override directory
    /// explicitly. Test-only: the live `capability_reload` command calls
    /// [`Self::reload_local`] on an existing registry rather than
    /// rebuilding one, so nothing in the product needs a
    /// build-from-scratch-with-an-explicit-dir entry point.
    #[cfg(test)]
    pub fn load_with_local_dir<P: AsRef<Path>>(local_dir: P) -> Self {
        let mut registry = Self::default();
        insert_built_ins(&mut registry.specs);
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
                    eprintln!("moddin: could not read {}: {error}", path.display());
                    continue;
                }
            };
            let mut spec = match serde_yaml::from_str::<CapabilitySpec>(&raw) {
                Ok(spec) => spec,
                Err(error) => {
                    eprintln!("moddin: could not parse {}: {error}", path.display());
                    continue;
                }
            };
            spec.origin = SpecOrigin::Local;
            let spec_id = spec.id.clone();
            let previous_origin = specs
                .insert(spec.id.clone(), spec)
                .map(|existing| existing.origin);
            match previous_origin {
                Some(SpecOrigin::BuiltIn) => {
                    eprintln!("moddin: local capability {spec_id} overrides the built-in")
                }
                Some(other) if other != SpecOrigin::Local => eprintln!(
                    "moddin: local capability {spec_id} overrides {} capability",
                    other.as_str()
                ),
                Some(SpecOrigin::Local) => {
                    eprintln!("moddin: duplicate local capability id {spec_id}, keeping first")
                }
                Some(SpecOrigin::Community) => {
                    eprintln!("moddin: local capability {spec_id} overrides community capability")
                }
                None => {}
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&CapabilitySpec> {
        self.specs.get(id)
    }

    /// Does at least one loaded spec declare this engine?
    ///
    /// The engine rule needs this and cannot compute it: a recipe's
    /// `supportedEngines` is what makes an engine *comparable*, and a
    /// game on an engine nothing declares (`doom-2016` on `idtech`)
    /// must not be gated on a comparison that has only one side.
    pub fn declares_engine(&self, engine: &str) -> bool {
        self.specs.values().any(|spec| {
            spec.supported_engines
                .iter()
                .any(|declared| declared == engine)
        })
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.specs.keys().map(String::as_str)
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

/// The per-install inputs that the command layer already holds and every
/// stage of the install pipeline needs: which game, the resolved recipe
/// config, and the two directories.
///
/// Bundled because five functions below took these five parameters in the
/// same order, and `install_directory` / `executable_directory` are
/// adjacent same-typed `&Path`s — a swapped pair still compiles and
/// silently points the backup scope at the wrong root.
#[derive(Clone, Copy)]
struct InstallScope<'a> {
    game_id: &'a str,
    game_name: &'a str,
    config: &'a ResolvedConfig,
    install_directory: &'a Path,
    executable_directory: &'a Path,
}

/// Internal install worker. Called by `run_install` (registry lookup)
/// and `run_install_with_spec` (community YAML the caller already
/// fetched + verified). Same transaction store, same step dispatch.
fn execute_install(
    spec: &CapabilitySpec,
    scope: &InstallScope<'_>,
) -> Result<InstallResult, String> {
    let InstallScope {
        game_id,
        game_name,
        config,
        install_directory,
        executable_directory,
    } = *scope;
    let step_context = StepContext {
        config,
        install_directory,
        executable_directory,
    };
    let mut step_results = Vec::new();
    let mut affected = Vec::new();

    // Plan each step's targets, back them up, then run the step. The
    // order matters: a file the step is about to overwrite has to be in
    // the backup *before* it is replaced, and a step that fails halfway
    // must still leave a record that can undo what already landed.
    let metadata = build_metadata(spec, config);
    let label = format!("Install {} for {}", spec.display_name, game_name);
    let mut record: Option<transaction::TransactionRecord> = None;
    let mut outside_root: Vec<String> = Vec::new();

    for step in &spec.install {
        let planned = builtin_steps::plan_step_targets(step, &step_context)?;

        // Only paths under the install directory are inside the game's
        // rollback scope. A recipe that writes elsewhere (an OpenXR
        // runtime manifest, a tray INI) is still allowed to do so, but
        // the user is told it is not covered by Undo.
        let mut in_scope = Vec::new();
        for path in planned {
            if path.starts_with(install_directory) {
                in_scope.push(path);
            } else {
                outside_root.push(path.to_string_lossy().into_owned());
            }
        }

        if !in_scope.is_empty() {
            record = Some(match record {
                Some(existing) => {
                    transaction::add_files_to_transaction(existing, install_directory, &in_scope)?
                }
                None => transaction::begin_file_set_transaction(
                    install_directory,
                    &in_scope,
                    &spec.id,
                    &label,
                    game_id,
                    metadata.clone(),
                )?,
            });
        }

        let result = match builtin_steps::execute_step(step, &step_context) {
            Ok(result) => result,
            Err(error) => {
                // Roll back whatever this install already wrote instead
                // of leaving orphaned files with no undo record.
                if let Some(prepared) = record {
                    let _ = transaction::restore_record(prepared);
                }
                return Err(error);
            }
        };
        affected.extend(result.affected_paths.clone());
        step_results.push(result);
    }

    let transaction = match record {
        Some(prepared) => Some(transaction::mark_applied(prepared)?),
        None => None,
    };

    if !outside_root.is_empty() {
        affected.extend(outside_root);
    }

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
    scope: &InstallScope<'_>,
    chain: &mut Vec<String>,
    installed_dependencies: &mut Vec<String>,
) -> Result<(), String> {
    let InstallScope {
        game_id,
        executable_directory,
        ..
    } = *scope;
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
                scope,
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
                        compatibility.detail.unwrap_or_else(|| {
                            "game executable outside the supported version range".to_owned()
                        })
                    ));
                }
            }
            execute_install(dependency_spec, scope)?;
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
        params.insert(
            "minVersion".to_owned(),
            serde_json::Value::String(min.clone()),
        );
    }
    if let Some(max) = &compatibility.max_exe_version {
        params.insert(
            "maxVersion".to_owned(),
            serde_json::Value::String(max.clone()),
        );
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
    scope: &InstallScope<'_>,
    force: bool,
) -> Result<InstallResult, String> {
    // `status: planned` means the recipe is a declared intention, not a
    // working installer. Without this guard a planned spec with no
    // install steps would "succeed", record no transaction, and leave
    // the user believing a mod was installed. `force` deliberately does
    // not bypass it: the compatibility gate is about the game build,
    // this is about the recipe not existing yet.
    if spec.status == "planned" {
        return Err(format!(
            "'{}' is still a planned recipe and has no working installer yet.",
            spec.display_name
        ));
    }
    let compatibility = evaluate_compatibility(
        spec,
        scope.executable_directory,
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
        scope,
        &mut chain,
        &mut installed_dependencies,
    )?;

    let mut result = execute_install(spec, scope)?;
    result.installed_dependencies = installed_dependencies;
    result.compatibility = compatibility;
    Ok(result)
}

/// Run the `install` section of a capability that lives in the
/// registry. Missing `dependencies` are auto-installed first (up to
/// [`MAX_DEPENDENCY_DEPTH`] levels); `force` bypasses the spec's
/// compatibility gate for this capability only.
fn run_install(
    registry: &CapabilityRegistry,
    capability_id: &str,
    scope: &InstallScope<'_>,
    force: bool,
) -> Result<InstallResult, String> {
    let spec = registry
        .get(capability_id)
        .ok_or_else(|| format!("Unknown capability id '{capability_id}'."))?;
    install_spec(registry, spec, scope, force)
}

/// Run the `install` section of a capability whose `CapabilitySpec`
/// was supplied directly (no registry lookup). Used by
/// `community_capability_install` after the caller has downloaded
/// and signature-verified the YAML. Dependencies resolve against the
/// regular registry (built-ins + local overrides).
fn run_install_with_spec(
    registry: &CapabilityRegistry,
    spec: &CapabilitySpec,
    scope: &InstallScope<'_>,
    force: bool,
) -> Result<InstallResult, String> {
    install_spec(registry, spec, scope, force)
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
///
/// Test-only: the `capability_evaluate` command goes through
/// [`evaluate_all_checks`] and the UI has no path that names one check
/// id, so this lookup wrapper is not compiled into the shipped binary.
/// It stays because the tests below are what pin the "search `checks`
/// *and* `verify`" behaviour that [`evaluate_all_checks`] relies on.
#[cfg(test)]
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

fn build_metadata(spec: &CapabilitySpec, config: &ResolvedConfig) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
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
        &InstallScope {
            game_id: &request.game_id,
            game_name: &request.game_name,
            config: &request.config,
            install_directory: &install_directory,
            executable_directory: &executable_directory,
        },
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
    /// Same player-facing line as [`CapabilitySpec::description`].
    /// The card shows this instead of the technical id.
    pub description: Option<String>,
    pub category: String,
    pub status: String,
    /// Provenance — drives the UI badge ("Verified", "Local",
    /// "Community"). See [`SpecOrigin`].
    pub origin: crate::capability::SpecOrigin,
    /// The engines this recipe declares, verbatim. Empty means
    /// *every* engine, not none — see
    /// [`CapabilitySpec::supported_engines`].
    pub supported_engines: Vec<String>,
    /// The engine verdict the backend already decided, so the card
    /// renders the reason instead of re-deriving the rule (ROADMAP
    /// F-09). A returned summary is always eligible; a
    /// [`crate::capability::EngineMatch::Mismatch`] never reaches the
    /// wire.
    pub engine_match: crate::capability::EngineMatch,
}

impl CapabilitySummary {
    fn from_spec(spec: &CapabilitySpec, scope: crate::capability::EngineScope<'_>) -> Self {
        Self {
            id: spec.id.clone(),
            display_name: spec.display_name.clone(),
            description: spec.description.clone(),
            category: spec.category.clone(),
            status: spec.status.clone(),
            origin: spec.origin,
            supported_engines: spec.supported_engines.clone(),
            engine_match: spec.engine_match(scope),
        }
    }
}

/// Summaries for one engine scope, with the mismatch gate applied.
///
/// Kept separate from the `#[tauri::command]` wrapper so the rule is
/// testable without an IPC payload, and so `capability_list` and
/// `capability_reload` cannot grow two different definitions of "the
/// list".
pub fn summaries_for_engine(
    registry: &CapabilityRegistry,
    game_engine: Option<&str>,
) -> Vec<CapabilitySummary> {
    let scope = crate::capability::EngineScope::resolve(game_engine, |engine| {
        registry.declares_engine(engine)
    });
    registry
        .ids()
        .filter_map(|id| {
            let spec = registry.get(id).expect("registry invariant");
            let summary = CapabilitySummary::from_spec(spec, scope);
            summary.engine_match.is_eligible().then_some(summary)
        })
        .collect()
}

/// Every capability the registry holds, with the engine verdict
/// attached.
///
/// `engine` is the selected game's `enginePreset`. It is optional on
/// purpose: a caller that does not know the game's engine gets the
/// whole list with `noGameEngine`, which is what an unscoped list means.
/// (Tauri passes `None` for a missing key rather than erroring, so the
/// existing no-argument callers keep working unchanged.)
#[tauri::command]
pub fn capability_list(engine: Option<String>) -> Vec<CapabilitySummary> {
    let registry = CapabilityRegistry::load();
    summaries_for_engine(&registry, engine.as_deref())
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
    // No engine argument: a reload is a catalogue refresh, not a
    // game-scoped query, so the list is unscoped and the gate stays
    // open. Same summaries as `capability_list` with no engine.
    Ok(summaries_for_engine(&registry, None))
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
    // The cached catalog is re-verified against the keyring here, inside
    // the install flow, and the capability is checked against
    // `revoked-ids.json` before its `downloadUrl` is read. Both fail
    // closed, so an install never runs off an entry the app has not just
    // proved is still signed and still live.
    let entry = crate::community_catalog::verified_entry_for_install(&request.capability_id)?;
    let download_url = entry.download_url.clone().ok_or_else(|| {
        format!(
            "Community catalog entry '{}' has no downloadUrl.",
            request.capability_id
        )
    })?;

    let (yaml_text, signed_by_text) =
        crate::community_catalog::fetch_capability_yaml(&download_url, None).await?;

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
        &InstallScope {
            game_id: &request.game_id,
            game_name: &request.game_name,
            config: &request.config,
            install_directory: &install_directory,
            executable_directory: &executable_directory,
        },
        request.force,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CheckSpec, EngineMatch, SpecOrigin};
    use serde_json::json;
    use std::fs;

    /// Build a resolved config from key/value pairs, matching how the
    /// Tauri command layer deserialises a request.
    fn config_with(pairs: &[(&str, &str)]) -> ResolvedConfig {
        ResolvedConfig {
            values: pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), json!(value)))
                .collect(),
        }
    }

    #[test]
    fn registry_loads_ofxr_bridge() {
        let registry = CapabilityRegistry::load();
        assert!(registry.get("ofxr-bridge").is_some());
        assert!(registry.ids().count() >= 1);
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
        let temp =
            std::env::temp_dir().join(format!("moddin-local-override-test-{}", std::process::id()));
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
        let temp =
            std::env::temp_dir().join(format!("moddin-local-add-test-{}", std::process::id()));
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
        let temp =
            std::env::temp_dir().join(format!("moddin-local-malformed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).expect("create temp dir");
        fs::write(
            temp.join("bad.yaml"),
            "this: is: not: valid: yaml: at: all:",
        )
        .expect("write malformed yaml");
        let registry = CapabilityRegistry::load_with_local_dir(&temp);
        // Built-ins unaffected by the malformed local file.
        assert!(registry.get("ofxr-bridge").is_some());

        let _ = fs::remove_dir_all(&temp);
    }

    // === Engine gate over the real registry (ROADMAP F-09) =========

    /// Built-ins only, so a developer's `%LOCALAPPDATA%` recipes cannot
    /// change what these assertions see.
    fn built_in_registry() -> CapabilityRegistry {
        let registry = CapabilityRegistry::load_with_local_dir(temp_root("engine-gate"));
        assert!(registry.ids().count() > 1, "expected the built-in recipes");
        registry
    }

    fn listed_ids(registry: &CapabilityRegistry, engine: Option<&str>) -> Vec<String> {
        let mut ids: Vec<String> = summaries_for_engine(registry, engine)
            .into_iter()
            .map(|summary| summary.id)
            .collect();
        ids.sort();
        ids
    }

    fn every_built_in_id(registry: &CapabilityRegistry) -> Vec<String> {
        let mut ids: Vec<String> = registry.ids().map(str::to_owned).collect();
        ids.sort();
        ids
    }

    /// The `enginePreset` a shipped catalogue file declares, read from
    /// the file rather than hard-coded so the Elden Ring test keeps
    /// testing what actually ships.
    fn shipped_engine_preset(game_file: &str) -> Option<String> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("src")
            .join("catalog")
            .join("games")
            .join(game_file);
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
        // Some catalogue files ship a UTF-8 BOM.
        let raw = raw.trim_start_matches('\u{feff}');
        let parsed: serde_yaml::Value = serde_yaml::from_str(raw)
            .unwrap_or_else(|error| panic!("could not parse {}: {error}", path.display()));
        parsed
            .get("enginePreset")
            .and_then(serde_yaml::Value::as_str)
            .map(str::to_owned)
    }

    #[test]
    fn a_re_engine_game_is_not_offered_uevr() {
        // The `dead-island-2` shape, one layer down: the recipe the
        // engine does not support must not reach the card list.
        let registry = built_in_registry();
        let listed = listed_ids(&registry, Some("re-engine"));
        assert!(!listed.iter().any(|id| id == "uevr"), "{listed:?}");
        assert!(!listed.iter().any(|id| id == "ue4ss"), "{listed:?}");
        // ...while the recipes that do target it still are.
        assert!(listed.iter().any(|id| id == "reframework"), "{listed:?}");
    }

    #[test]
    fn an_unreal_game_is_not_offered_a_re_engine_or_unity_recipe() {
        let registry = built_in_registry();
        let listed = listed_ids(&registry, Some("unreal5"));
        assert!(listed.iter().any(|id| id == "uevr"), "{listed:?}");
        assert!(listed.iter().any(|id| id == "ue4ss"), "{listed:?}");
        assert!(!listed.iter().any(|id| id == "reframework"), "{listed:?}");
        assert!(!listed.iter().any(|id| id == "bepinex"), "{listed:?}");
    }

    #[test]
    fn every_returned_summary_carries_the_verdict_that_kept_it() {
        let registry = built_in_registry();
        for engine in ["unreal5", "redengine", "re-engine", "unity", "idtech"] {
            for summary in summaries_for_engine(&registry, Some(engine)) {
                assert!(
                    summary.engine_match.is_eligible(),
                    "{} was gated out and must not be listed",
                    summary.id
                );
            }
        }
    }

    #[test]
    fn an_unscoped_list_returns_every_recipe() {
        // What the three existing no-argument callers get: the whole
        // registry, nothing gated, because no engine was named.
        let registry = built_in_registry();
        let summaries = summaries_for_engine(&registry, None);
        assert_eq!(listed_ids(&registry, None), every_built_in_id(&registry));
        assert!(summaries
            .iter()
            .all(|summary| summary.engine_match == EngineMatch::NoGameEngine));
    }

    #[test]
    fn an_engine_no_recipe_declares_keeps_every_card() {
        // `idtech` is a real preset (`doom-2016` uses it) and nothing
        // Moddin ships claims id Tech. Gating on a comparison with only
        // one side would empty the game's card list.
        let registry = built_in_registry();
        assert!(!registry.declares_engine("idtech"));
        let listed = listed_ids(&registry, Some("idtech"));
        assert_eq!(listed, every_built_in_id(&registry));
        assert!(summaries_for_engine(&registry, Some("idtech"))
            .iter()
            .all(|summary| matches!(summary.engine_match, EngineMatch::UnknownGameEngine { .. })));
    }

    #[test]
    fn elden_ring_declares_no_engine_preset_so_nothing_is_gated() {
        // Elden Ring ships no `enginePreset` and it is the app's
        // flagship VR title. Its UEVR card — the one combination the
        // project is known for — must survive the gate, and the
        // catalogue file itself is what decides that, so it is read
        // from disk rather than asserted here.
        assert_eq!(
            shipped_engine_preset("elden-ring.yaml"),
            None,
            "elden-ring.yaml started declaring an engine; this test's premise changed"
        );
        let registry = built_in_registry();
        let engine = shipped_engine_preset("elden-ring.yaml");
        let listed = listed_ids(&registry, engine.as_deref());
        assert_eq!(listed, every_built_in_id(&registry));
        assert!(listed.iter().any(|id| id == "uevr"), "{listed:?}");
    }

    #[test]
    fn a_shipped_game_with_an_engine_preset_gets_that_engines_cards() {
        // The other half of the Elden Ring case, read from the
        // catalogue: `dead-island-2` declares `unreal5`, so the RE
        // Engine-only recipe must not reach its list.
        let engine = shipped_engine_preset("dead-island-2.yaml");
        assert_eq!(engine.as_deref(), Some("unreal5"));
        let registry = built_in_registry();
        let listed = listed_ids(&registry, engine.as_deref());
        assert!(!listed.iter().any(|id| id == "reframework"), "{listed:?}");
        assert!(listed.iter().any(|id| id == "uevr"), "{listed:?}");
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
            serde_json::Value::String(executable.join("middle.txt").to_string_lossy().into_owned()),
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
            &InstallScope {
                game_id,
                game_name: "Dep Order Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
            false,
        )
        .expect("install with auto-installed dependencies");

        // Transitive dependency installs first, then its dependent.
        assert_eq!(
            result.installed_dependencies,
            vec!["dep-base", "dep-middle"]
        );
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
                executable
                    .join("present.txt")
                    .to_string_lossy()
                    .into_owned(),
            ),
        );
        config.values.insert(
            "markerRequester".to_owned(),
            serde_json::Value::String(
                executable
                    .join("requester.txt")
                    .to_string_lossy()
                    .into_owned(),
            ),
        );

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let game_id = "dep-skip-game";
        run_install(
            &registry,
            "dep-present",
            &InstallScope {
                game_id,
                game_name: "Dep Skip Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
            false,
        )
        .expect("install the dependency on its own");

        let result = run_install(
            &registry,
            "dep-requester",
            &InstallScope {
                game_id,
                game_name: "Dep Skip Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
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
            &InstallScope {
                game_id: "cycle-game",
                game_name: "Cycle Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
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
            &InstallScope {
                game_id: "cycle-game",
                game_name: "Cycle Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
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
            &InstallScope {
                game_id: "depth-game",
                game_name: "Depth Game",
                config: &ResolvedConfig::default(),
                install_directory: &work,
                executable_directory: &executable,
            },
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
            &InstallScope {
                game_id: "missing-game",
                game_name: "Missing Game",
                config: &ResolvedConfig::default(),
                install_directory: &work,
                executable_directory: &executable,
            },
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
            serde_json::Value::String(executable.join("gated.txt").to_string_lossy().into_owned()),
        );

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let error = run_install(
            &registry,
            "gated-mod",
            &InstallScope {
                game_id: "gate-game",
                game_name: "Gate Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
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
            serde_json::Value::String(executable.join("gated.txt").to_string_lossy().into_owned()),
        );

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let result = run_install(
            &registry,
            "gated-mod",
            &InstallScope {
                game_id: "gate-game",
                game_name: "Gate Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
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
            first
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("game.exe"),
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
            executable_dir,
        })
        .expect_err("an unknown id is an error, not a silent None");
        assert!(
            error.contains("Unknown capability id"),
            "the error names the id: {error}"
        );

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn a_planned_recipe_refuses_to_install() {
        let appdata = IsolatedAppdata::new("planned-recipe");
        let caps = appdata.dir.join("Moddin").join("capabilities");
        write_local_capability(
            &caps,
            "planned-mod",
            "id: planned-mod\ndisplayName: Planned mod\ncategory: qol\nstatus: planned\nsafetyNotes:\n  - Still being written.\n",
        );

        let work = temp_root("planned-recipe-work");
        let executable = work.join("exe");
        fs::create_dir_all(&executable).expect("exe dir");
        let registry = CapabilityRegistry::load_with_local_dir(&caps);

        // A planned spec has no install steps. Without the guard the run
        // would succeed, touch nothing, and record no transaction — the
        // user would see "installed" and have nothing to undo.
        let error = run_install(
            &registry,
            "planned-mod",
            &InstallScope {
                game_id: "planned-game",
                game_name: "Planned Game",
                config: &ResolvedConfig::default(),
                install_directory: &work,
                executable_directory: &executable,
            },
            false,
        )
        .expect_err("a planned recipe cannot be installed");
        assert!(
            error.contains("still a planned recipe"),
            "the error says why: {error}"
        );

        // `force` is about the game build, not about a recipe that does
        // not exist yet, so it must not open this door.
        let forced = run_install(
            &registry,
            "planned-mod",
            &InstallScope {
                game_id: "planned-game",
                game_name: "Planned Game",
                config: &ResolvedConfig::default(),
                install_directory: &work,
                executable_directory: &executable,
            },
            true,
        )
        .expect_err("force does not bypass the planned guard");
        assert!(forced.contains("still a planned recipe"), "{forced}");

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

    /// Guards the bug class behind "adding a mod is one YAML file".
    ///
    /// Every shipped recipe that is `available` and actually installs
    /// something has to read its payload from a local path that a
    /// previous `download-file` step produced. Pointing `extract-zip` at
    /// a URL — which every recipe used to do — made the whole install
    /// chain fail on its first step, at runtime, with a file-not-found
    /// the user had no way to act on.
    #[test]
    fn every_available_built_in_recipe_reads_its_payload_from_the_download_cache() {
        for raw in BUILT_IN_YAML {
            let spec: CapabilitySpec = serde_yaml::from_str(raw)
                .unwrap_or_else(|error| panic!("built-in capability YAML does not parse: {error}"));
            if spec.status != "available" || spec.install.is_empty() {
                continue;
            }

            let extracts_anything = spec.install.iter().any(|step| step.kind == "extract-zip");
            if !extracts_anything {
                continue;
            }

            assert!(
                spec.install.iter().any(|step| step.kind == "download-file"),
                "'{}' extracts an archive but never downloads one",
                spec.id
            );

            for step in spec
                .install
                .iter()
                .filter(|step| step.kind == "extract-zip")
            {
                let reference = step
                    .params
                    .get("archivePath")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
                    .or_else(|| {
                        step.params
                            .get("archivePathField")
                            .and_then(|value| value.as_str())
                            .map(|field| {
                                format!(
                                    "{field} -> {}",
                                    spec.config_schema
                                        .iter()
                                        .find(|declared| declared.name == field)
                                        .map(|declared| declared.field_type.clone())
                                        .unwrap_or_else(|| "missing field".to_owned())
                                )
                            })
                    })
                    .expect("extract-zip needs archivePath or archivePathField");

                assert!(
                    !reference.contains("://"),
                    "'{}' points extract-zip at a URL ({reference}); \
                     a download-file step has to fetch it into Moddin's cache first",
                    spec.id
                );
            }
        }
    }

    /// The transaction has to be opened *before* the step that overwrites
    /// a file runs. Opened afterwards — as it used to be — the backup
    /// holds the mod's own output, so Undo restores the mod instead of
    /// the player's file and the original is gone for good.
    #[test]
    fn an_install_backs_up_an_existing_game_file_before_overwriting_it() {
        let appdata = IsolatedAppdata::new("overwrite-backup");
        let caps = appdata.dir.join("Moddin").join("capabilities");
        write_local_capability(
            &caps,
            "overwriter",
            r#"
id: overwriter
displayName: Overwriter
category: qol
status: available
configSchema:
  - name: target
    type: string
    required: true
install:
  - kind: write-text-file
    description: Overwrite a file the game already shipped with.
    params:
      pathField: target
      template: replaced
"#,
        );

        let work = temp_root("overwrite-backup-work");
        let executable = work.join("game");
        fs::create_dir_all(&executable).expect("game dir");
        let victim = executable.join("settings.ini");
        fs::write(&victim, b"original").expect("seed original");

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let config = config_with(&[("target", "settings.ini")]);

        let result = run_install(
            &registry,
            "overwriter",
            &InstallScope {
                game_id: "overwrite-game",
                game_name: "Overwrite Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
            false,
        )
        .expect("install runs");
        assert!(
            result.transaction.is_some(),
            "an install that touches a file records a transaction"
        );
        assert_eq!(
            fs::read_to_string(&victim).expect("read victim"),
            "replaced",
            "the step really did overwrite the file"
        );

        run_uninstall(
            &registry,
            "overwriter",
            "overwrite-game",
            "Overwrite Game",
            &work,
        )
        .expect("uninstall runs");

        assert_eq!(
            fs::read_to_string(&victim).expect("read victim after undo"),
            "original",
            "Undo must restore the player's original content, not the mod's output"
        );

        let _ = fs::remove_dir_all(&work);
    }

    /// A step that fails halfway must not leave the files the earlier
    /// steps already wrote behind with no rollback record. Before the
    /// runner was reordered, `?` returned out of the step loop before
    /// any transaction existed, so those writes were simply orphaned.
    #[test]
    fn a_failed_step_rolls_back_what_the_install_already_wrote() {
        let appdata = IsolatedAppdata::new("partial-failure");
        let caps = appdata.dir.join("Moddin").join("capabilities");
        write_local_capability(
            &caps,
            "half-writer",
            r#"
id: half-writer
displayName: Half writer
category: qol
status: available
install:
  - kind: write-text-file
    description: Succeeds.
    params:
      path: first.ini
      template: written
  - kind: file-delete
    description: Fails on purpose — there is no such file to delete.
    params:
      path: does-not-exist.ini
"#,
        );

        let work = temp_root("partial-failure-work");
        let executable = work.join("game");
        fs::create_dir_all(&executable).expect("game dir");

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let error = run_install(
            &registry,
            "half-writer",
            &InstallScope {
                game_id: "partial-game",
                game_name: "Partial Game",
                config: &ResolvedConfig::default(),
                install_directory: &work,
                executable_directory: &executable,
            },
            false,
        )
        .expect_err("the second step fails");

        assert!(error.contains("does-not-exist.ini"), "{error}");
        assert!(
            !executable.join("first.ini").exists(),
            "the file the first step wrote must be rolled back, not orphaned"
        );

        let _ = fs::remove_dir_all(&work);
    }

    /// Step paths are rendered from config, so `..` in a config value
    /// would otherwise let a recipe write anywhere on the machine.
    #[test]
    fn a_step_cannot_write_outside_the_install_root_without_saying_so() {
        let appdata = IsolatedAppdata::new("escape-root");
        let caps = appdata.dir.join("Moddin").join("capabilities");
        write_local_capability(
            &caps,
            "escaper",
            r#"
id: escaper
displayName: Escaper
category: qol
status: available
configSchema:
  - name: target
    type: string
    required: true
install:
  - kind: write-text-file
    description: Try to climb out of the game folder.
    params:
      pathField: target
      template: owned
"#,
        );

        let work = temp_root("escape-root-work");
        let executable = work.join("game");
        fs::create_dir_all(&executable).expect("game dir");
        let outside = work.join("hijacked.ini");

        let registry = CapabilityRegistry::load_with_local_dir(&caps);
        let config = config_with(&[("target", "../hijacked.ini")]);

        let error = run_install(
            &registry,
            "escaper",
            &InstallScope {
                game_id: "escape-game",
                game_name: "Escape Game",
                config: &config,
                install_directory: &work,
                executable_directory: &executable,
            },
            false,
        )
        .expect_err("traversal out of the install root is refused");

        assert!(error.contains("escapes the install directory"), "{error}");
        assert!(!outside.exists(), "nothing was written outside the root");

        let _ = fs::remove_dir_all(&work);
    }
}
