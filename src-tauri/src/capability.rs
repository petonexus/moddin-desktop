//! Universal capability contract.
//!
//! A **capability** is a data-driven recipe that declares everything a
//! tool needs to know about itself: what files / processes / registry
//! keys it owns, what checks to run before and after install, what
//! concrete steps to execute, and which fields the recipe config must
//! carry.
//!
//! The goal is that adding a new mod to Moddin is a **single YAML file**
//! under `src-tauri/capabilities/<id>.yaml` plus a matching entry in
//! `src/catalog/games/<game>.yaml`. The agent (LLM) then only needs to
//! know the capability id and a typed config to install it on a
//! chosen game — no Rust change required.
//!
//! Every step / check references one of the registered kinds declared
//! in [`crate::builtin_steps`] and [`crate::builtin_checks`]. New kinds
//! are added by implementing the matching dispatch arm there.
//!
//! ## See also
//!
//! - [`docs/CAPABILITY-CONTRACT.md`](../docs/CAPABILITY-CONTRACT.md) —
//!   the authoritative specification.
//! - `src-tauri/capabilities/ofxr-bridge.yaml` — a complete sample.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

/// Where a [`CapabilitySpec`] came from. Determines how the UI badges
/// it, what activity log level it carries, and whether the install
/// prompts for confirmation.
///
/// * `BuiltIn` — shipped inside the app binary (Rust `include_str!`).
/// * `Local` — a YAML file under `%LOCALAPPDATA%\Moddin\capabilities\`.
///   The user dropped it themselves, so we trust it without
///   prompting. Same kind allow-list as BuiltIn.
/// * `Community` — fetched from the public community catalog
///   ([`petonexus/moddin-community-capabilities`](https://github.com/petonexus/moddin-community-capabilities))
///   and verified against the maintainer signing key. The runner
///   refuses to load unsigned community capabilities without a UI
///   confirmation (handled in the catalog layer, not here).
///
/// Adding a new variant requires updating the TypeScript mirror in
/// `src/types/capability.ts`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SpecOrigin {
    BuiltIn,
    Local,
    Community,
}

impl SpecOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpecOrigin::BuiltIn => "builtIn",
            SpecOrigin::Local => "local",
            SpecOrigin::Community => "community",
        }
    }
}

impl Default for SpecOrigin {
    fn default() -> Self {
        SpecOrigin::BuiltIn
    }
}

/// Top-level capability recipe. Parsed from a YAML file under
/// `src-tauri/capabilities/<id>.yaml` or
/// `%LOCALAPPDATA%\Moddin\capabilities\<id>.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySpec {
    /// Stable id, e.g. `"ofxr-bridge"`. Must be unique across all loaded
    /// specs (the runner asserts this at load time).
    pub id: String,
    pub display_name: String,
    /// One sentence, written for a person rather than for a recipe
    /// author: what the mod does for the player, not how it is
    /// installed. Shown as the card's supporting line. Absent means the
    /// UI falls back to the technical id rather than inventing copy.
    #[serde(default)]
    pub description: Option<String>,
    pub category: String,
    pub status: String,
    #[serde(default)]
    pub supported_engines: Vec<String>,
    /// Ids of other capabilities that must be installed before this
    /// one. The runner installs every missing dependency (recursively,
    /// with cycle and depth guards) before executing `install`, using
    /// the same transaction store the uninstall path consults to decide
    /// whether a capability is active for the game.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Optional game-build compatibility window. When present, the
    /// runner evaluates the game executable's `FileVersion` before
    /// installing (unless the request passes `force: true`) and also
    /// reports it as a structured check from `capability_evaluate`.
    /// Absent means "compatible with every build".
    #[serde(default)]
    pub compatibility: Option<CompatibilitySpec>,
    #[serde(default)]
    pub checks: Vec<CheckSpec>,
    #[serde(default)]
    pub install: Vec<StepSpec>,
    #[serde(default)]
    pub uninstall: Vec<StepSpec>,
    #[serde(default)]
    pub verify: Vec<CheckSpec>,
    #[serde(default)]
    pub safety_notes: Vec<String>,
    #[serde(default)]
    pub config_schema: Vec<ConfigFieldSpec>,
    /// Where the spec came from. Not serialised to / parsed from
    /// YAML — the runner sets it after loading. Defaults to `BuiltIn`
    /// so older YAMLs keep their original provenance.
    #[serde(default, skip_deserializing)]
    pub origin: SpecOrigin,
}

/// Game-build compatibility window declared by a capability. The
/// runner reads the game executable's `FileVersion` (via PowerShell,
/// the same channel the rest of the codebase uses for version probes)
/// and compares it against the declared range using the tolerant
/// natural-order comparator from [`crate::updates`].
///
/// All fields are optional; a block with no constraint at all is
/// treated as "compatible with everything" and is not evaluated.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilitySpec {
    /// Path of the game executable whose `FileVersion` is checked,
    /// relative to the game's executable directory (same resolution
    /// `file-exists` uses).
    pub game_exe: Option<String>,
    /// Lowest accepted `FileVersion`, inclusive.
    pub min_exe_version: Option<String>,
    /// Highest accepted `FileVersion`, inclusive.
    pub max_exe_version: Option<String>,
    /// Exact `FileVersion`s that must never run with this capability.
    #[serde(default)]
    pub blocked_exe_versions: Vec<String>,
}

impl CompatibilitySpec {
    /// A block only constrains the install when it names an exe and at
    /// least one bound; an empty block must not gate anything.
    pub fn has_constraints(&self) -> bool {
        self.game_exe.is_some()
            && (self.min_exe_version.is_some()
                || self.max_exe_version.is_some()
                || !self.blocked_exe_versions.is_empty())
    }
}

/// Declarative description of a single check the runner can evaluate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSpec {
    pub id: String,
    pub label: String,
    /// Kind understood by the runner's [`crate::builtin_checks`]
    /// dispatcher. Unknown kinds fail the load (validated centrally).
    pub kind: String,
    #[serde(default)]
    pub params: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub severity: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub description: Option<String>,
}

fn default_category() -> String {
    "modulespecific".to_owned()
}

/// Declarative description of a single install / uninstall step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepSpec {
    /// Kind understood by [`crate::builtin_steps`].
    pub kind: String,
    #[serde(default)]
    pub params: BTreeMap<String, JsonValue>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Field declared in `configSchema`. The runner surfaces this to the
/// UI so it can render a typed input form instead of freeform JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFieldSpec {
    pub name: String,
    /// Type tag: `string`, `number`, `boolean`, `url`, `sha256`,
    /// `path`, `enum`. The runner treats this as a UI hint, not a hard
    /// validator.
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<JsonValue>,
    #[serde(default)]
    pub enum_values: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Resolved config passed alongside a capability invocation. The runner
/// looks up values by name when evaluating checks / steps.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfig {
    pub values: BTreeMap<String, JsonValue>,
}

impl ResolvedConfig {
    pub fn get(&self, name: &str) -> Option<&JsonValue> {
        self.values.get(name)
    }

    pub fn get_string(&self, name: &str) -> Option<String> {
        self.get(name).and_then(|value| value.as_str().map(str::to_owned))
    }

    pub fn get_u64(&self, name: &str) -> Option<u64> {
        self.get(name).and_then(|value| value.as_u64())
    }

    pub fn get_bool(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(|value| value.as_bool())
    }
}

/// Severity strings the capability schema reuses from the catalog.
pub mod severity {
    pub const INFO: &str = "info";
    pub const WARNING: &str = "warning";
    pub const BLOCKER: &str = "blocker";
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolves_string_and_bool_from_config() {
        let config = ResolvedConfig {
            values: BTreeMap::from([
                ("downloadUrl".to_owned(), json!("https://example.com/foo.zip")),
                ("forceReinstall".to_owned(), json!(true)),
            ]),
        };
        assert_eq!(
            config.get_string("downloadUrl").as_deref(),
            Some("https://example.com/foo.zip")
        );
        assert_eq!(config.get_bool("forceReinstall"), Some(true));
        assert!(config.get_string("missing").is_none());
    }

    #[test]
    fn severity_constants_match_catalog() {
        assert_eq!(severity::INFO, "info");
        assert_eq!(severity::WARNING, "warning");
        assert_eq!(severity::BLOCKER, "blocker");
    }

    #[test]
    fn spec_without_new_fields_defaults_to_no_constraints() {
        let spec: CapabilitySpec = serde_yaml::from_str(
            r#"
id: bare-capability
displayName: Bare
category: vr
status: available
"#,
        )
        .expect("minimal spec parses");
        assert!(spec.dependencies.is_empty());
        assert!(spec.compatibility.is_none());
    }

    #[test]
    fn spec_parses_dependencies_and_compatibility() {
        let spec: CapabilitySpec = serde_yaml::from_str(
            r#"
id: dependent-mod
displayName: Dependent mod
category: graphics
status: available
dependencies:
  - optiscaler
  - ofxr-bridge
compatibility:
  gameExe: Game/Binaries/Win64/Game-Win64-Shipping.exe
  minExeVersion: 1.16.0
  maxExeVersion: 1.17.9.9
  blockedExeVersions:
    - 1.16.3.0
"#,
        )
        .expect("spec with dependencies + compatibility parses");
        assert_eq!(spec.dependencies, vec!["optiscaler", "ofxr-bridge"]);
        let compatibility = spec.compatibility.expect("compatibility block parsed");
        assert_eq!(
            compatibility.game_exe.as_deref(),
            Some("Game/Binaries/Win64/Game-Win64-Shipping.exe")
        );
        assert_eq!(compatibility.min_exe_version.as_deref(), Some("1.16.0"));
        assert_eq!(compatibility.max_exe_version.as_deref(), Some("1.17.9.9"));
        assert_eq!(compatibility.blocked_exe_versions, vec!["1.16.3.0"]);
        assert!(compatibility.has_constraints());
    }

    #[test]
    fn empty_compatibility_block_has_no_constraints() {
        let compatibility = CompatibilitySpec::default();
        assert!(!compatibility.has_constraints());
        let without_exe = CompatibilitySpec {
            min_exe_version: Some("1.0.0".to_owned()),
            ..CompatibilitySpec::default()
        };
        assert!(!without_exe.has_constraints());
    }
}