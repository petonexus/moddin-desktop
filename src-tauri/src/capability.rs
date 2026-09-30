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
    /// Engine ids this capability is built for. An **absent or empty**
    /// list means *every* engine, not *no* engine — the field is
    /// optional, and `moddin-agent/templates/extract-zip.yaml`
    /// documents `supportedEngines: []` as "engine-agnostic". Reading
    /// it the other way round would make every community and local
    /// recipe that omits the key invisible on every game. Compare with
    /// [`CapabilitySpec::engine_match`]; the TypeScript mirror of that
    /// decision lives on [`EngineMatch`].
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

/// Every engine id the shipped catalogue knows about.
///
/// One vocabulary, declared in four places: here, the `EngineId` union
/// in `src/types/capability.ts`, the `supportedEngines.items.enum` in
/// `moddin-agent/schema/capability.schema.json`, and the preset files
/// themselves under `src/catalog/engines/`. They are compared by
/// `scripts/check-capability-kind-parity.mjs`, because an id added to
/// one of them and not the others is exactly how a recipe stops being
/// offered to the game it was written for.
///
/// An id outside this list is never rejected. It resolves to
/// [`EngineMatch::UnknownGameEngine`], which gates nothing — a typo in
/// one file must not silently disable the gate everywhere.
pub const KNOWN_ENGINES: &[&str] = &["idtech", "re-engine", "redengine", "unity", "unreal5"];

/// Whether `engine` is part of the shared vocabulary.
pub fn is_known_engine(engine: &str) -> bool {
    KNOWN_ENGINES.contains(&engine)
}

/// The engine a `capability_list` call is scoped to, already resolved
/// against what the registry actually knows.
///
/// The catalogue is two data sets that meet here — a game's
/// `enginePreset` (frontend YAML) and a recipe's `supportedEngines`
/// (backend YAML) — and the join can fail in three different ways. Only
/// one of them is a real mismatch, and getting the other two wrong
/// hides working mods, so the distinction is a type rather than a
/// boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineScope<'a> {
    /// The game declares no `enginePreset` at all. `elden-ring.yaml`
    /// does not, and it is the app's flagship VR title: gating it on an
    /// engine nobody declared would hide UEVR from the game people most
    /// use UEVR with.
    Unscoped,
    /// A game named an engine the catalogue has no opinion about —
    /// either the id is not in [`KNOWN_ENGINES`], or no loaded recipe
    /// declares it (`doom-2016` is `idtech`; nothing Moddin ships
    /// claims id Tech). There is no second side to compare against, so
    /// nothing is gated.
    Unopinionated(&'a str),
    /// A real gate: a known engine that at least one recipe declares.
    Engine(&'a str),
}

impl<'a> EngineScope<'a> {
    /// Resolve what the caller named into a scope. `covered` answers
    /// "does any loaded recipe declare this engine?" — a registry
    /// fact, so the contract takes it as a parameter instead of
    /// guessing.
    pub fn resolve(game_engine: Option<&'a str>, covered: impl Fn(&str) -> bool) -> Self {
        let Some(engine) = game_engine.map(str::trim).filter(|engine| !engine.is_empty())
        else {
            return Self::Unscoped;
        };
        if !is_known_engine(engine) || !covered(engine) {
            return Self::Unopinionated(engine);
        }
        Self::Engine(engine)
    }
}

/// How one capability spec relates to the engine of the game it is
/// being offered for — the verdict F-09 needed and never had.
/// `supportedEngines` used to be parsed and then ignored, so an Unreal
/// 5 game was offered RE Engine capabilities and a RE Engine game was
/// offered UEVR.
///
/// Serialised on the wire with a `verdict` tag so the card can render
/// the reason instead of re-deriving the rule in TypeScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", rename_all = "camelCase")]
pub enum EngineMatch {
    /// The game declares no engine. Nothing is gated.
    NoGameEngine,
    /// The recipe declares no `supportedEngines` (absent or empty), so
    /// it applies to every engine.
    EngineAgnostic,
    /// The game named an engine no loaded recipe declares, or one
    /// outside [`KNOWN_ENGINES`]. Nothing is gated.
    UnknownGameEngine {
        /// The id the game named, verbatim.
        engine: String,
    },
    /// The recipe declares the game's engine.
    Supported,
    /// The recipe declares engines and the game's engine is not one of
    /// them. The only verdict that gates.
    Mismatch {
        /// The engines the recipe does declare, for the explanation.
        supported: Vec<String>,
    },
}

impl EngineMatch {
    /// Whether this capability may be offered for the game. Every
    /// verdict except a real mismatch is eligible; the fail-open cases
    /// are the point. A card must still be able to say *why* it is
    /// shown, which is why the verdict travels with the summary rather
    /// than the caller recomputing it.
    pub fn is_eligible(&self) -> bool {
        !matches!(self, Self::Mismatch { .. })
    }
}

impl CapabilitySpec {
    /// The one place the engine rule lives. The UI mirrors the verdict,
    /// not the rule.
    pub fn engine_match(&self, scope: EngineScope<'_>) -> EngineMatch {
        let engine = match scope {
            EngineScope::Unscoped => return EngineMatch::NoGameEngine,
            EngineScope::Unopinionated(engine) => {
                return EngineMatch::UnknownGameEngine {
                    engine: engine.to_owned(),
                }
            }
            EngineScope::Engine(engine) => engine,
        };

        let declared: Vec<&str> = self
            .supported_engines
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|engine| !engine.is_empty())
            .collect();
        if declared.is_empty() {
            return EngineMatch::EngineAgnostic;
        }
        if declared.contains(&engine) {
            return EngineMatch::Supported;
        }
        EngineMatch::Mismatch {
            supported: declared.into_iter().map(str::to_owned).collect(),
        }
    }
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

    // === Engine gate (ROADMAP F-09) =================================

    fn spec(engines: &[&str]) -> CapabilitySpec {
        let list = if engines.is_empty() {
            "supportedEngines: []\n".to_owned()
        } else {
            let mut list = "supportedEngines:\n".to_owned();
            for engine in engines {
                list.push_str(&format!("  - \"{engine}\"\n"));
            }
            list
        };
        serde_yaml::from_str(&format!(
            "id: probe\ndisplayName: Probe\ncategory: vr\nstatus: available\n{list}"
        ))
        .expect("probe spec parses")
    }

    /// Every engine the gate can act on. A real `capability_list` call
    /// supplies this from the registry; the contract only needs to know
    /// that the engine has at least one recipe behind it.
    fn every_engine_covered(_engine: &str) -> bool {
        true
    }

    #[test]
    fn empty_supported_engines_means_every_engine() {
        // The convention the whole gate rests on. `[]` and an absent
        // key are the same thing, and both mean "engine-agnostic" —
        // see the `supported_engines` doc comment and the parity guard,
        // which holds the other three declarations of it in line.
        for engines in [&[][..], &["", "  "][..]] {
            let spec = spec(engines);
            let scope = EngineScope::resolve(Some("unreal5"), every_engine_covered);
            assert_eq!(
                spec.engine_match(scope),
                EngineMatch::EngineAgnostic,
                "supportedEngines: {engines:?} must not gate anything"
            );
            assert!(spec.engine_match(scope).is_eligible());
        }
    }

    #[test]
    fn a_capability_is_supported_by_the_engine_it_lists() {
        let uevr = spec(&["unreal5"]);
        let scope = EngineScope::resolve(Some("unreal5"), every_engine_covered);
        assert_eq!(uevr.engine_match(scope), EngineMatch::Supported);
        assert!(uevr.engine_match(scope).is_eligible());
    }

    #[test]
    fn an_unreal_game_is_not_offered_a_re_engine_capability() {
        // The defect F-09 describes, in the shape the validator also
        // catches in the data: a recipe that targets `re-engine` is a
        // mismatch for a game on `unreal5`, and only this verdict
        // gates.
        let reframework = spec(&["re-engine"]);
        let scope = EngineScope::resolve(Some("unreal5"), every_engine_covered);
        let verdict = reframework.engine_match(scope);
        assert_eq!(
            verdict,
            EngineMatch::Mismatch {
                supported: vec!["re-engine".to_owned()]
            }
        );
        assert!(!verdict.is_eligible());
    }

    #[test]
    fn a_game_without_an_engine_preset_is_never_gated() {
        // Elden Ring. `supportedEngines: [unreal5]` on UEVR and no
        // `enginePreset` on the game: nothing to compare, so UEVR stays
        // on offer. Hiding it would be a regression dressed as a
        // feature.
        let uevr = spec(&["unreal5"]);
        let scope = EngineScope::resolve(None, every_engine_covered);
        assert_eq!(uevr.engine_match(scope), EngineMatch::NoGameEngine);
        assert!(uevr.engine_match(scope).is_eligible());
    }

    #[test]
    fn an_engine_no_recipe_declares_is_never_gated() {
        // DOOM's `idtech`: a real preset, but no shipped recipe claims
        // it. There is no second side to the comparison, so the gate
        // stays open instead of emptying the game's card list.
        let optiscaler = spec(&["unreal5", "redengine", "unity"]);
        let scope = EngineScope::resolve(Some("idtech"), |_| false);
        let verdict = optiscaler.engine_match(scope);
        assert_eq!(
            verdict,
            EngineMatch::UnknownGameEngine {
                engine: "idtech".to_owned()
            }
        );
        assert!(verdict.is_eligible());
    }

    #[test]
    fn an_engine_outside_the_shared_vocabulary_is_never_gated() {
        let optiscaler = spec(&["unreal5"]);
        let scope = EngineScope::resolve(Some("unreal-engine-5"), every_engine_covered);
        let verdict = optiscaler.engine_match(scope);
        assert_eq!(
            verdict,
            EngineMatch::UnknownGameEngine {
                engine: "unreal-engine-5".to_owned()
            }
        );
        assert!(verdict.is_eligible());
    }

    #[test]
    fn a_blank_engine_is_treated_as_no_engine() {
        for blank in ["", "   "] {
            assert_eq!(
                EngineScope::resolve(Some(blank), every_engine_covered),
                EngineScope::Unscoped
            );
        }
    }

    #[test]
    fn engine_match_travels_as_a_tagged_verdict_the_card_can_explain() {
        let mismatch = spec(&["unreal5"]).engine_match(EngineScope::Engine("redengine"));
        assert_eq!(
            serde_json::to_value(&mismatch).expect("serialise mismatch"),
            serde_json::json!({ "verdict": "mismatch", "supported": ["unreal5"] })
        );
        assert_eq!(
            serde_json::to_value(spec(&[]).engine_match(EngineScope::Engine("unity")))
                .expect("serialise agnostic"),
            serde_json::json!({ "verdict": "engineAgnostic" })
        );
    }
}