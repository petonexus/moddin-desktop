//! Curated bundles of capabilities — "collections" — loaded from YAML.
//!
//! Two directories, one vocabulary. The shipped collections under
//! `src-tauri/collections/` are compiled into the binary; a user can drop
//! their own into `%LOCALAPPDATA%\Moddin\collections\`. A local file
//! carrying a built-in's id shadows it, and the shadowing is *reported*,
//! not silent — the same contract [`crate::capability_runner::CapabilityRegistry`]
//! has for recipes, and the reason it is a report rather than a log line is
//! that a silently replaced collection is a set of capabilities the user
//! did not choose and cannot explain.
//!
//! A collection is not an install path. It is a list: this loader
//! resolves it against the capability registry and returns a preview, and
//! the frontend walks that preview through the existing
//! `community_capability_install` command, one member at a time, with the
//! preflight, transaction and rollback that command already has. The
//! `feat/agent-mcp` branch carried this loader next to a 1,127-line
//! session runner with its own `collection_install` command, its own
//! abort/resume and its own YAML writer; none of that came over, and
//! registering any of it would put a second install path back into a tree
//! that has spent a week deleting them.
//!
//! What the loader *does* refuse is a collection that could not be
//! installed: a member that is not a registered capability, a member that
//! is a **community** recipe (which is signature-checked when it installs,
//! not when a set is previewed, so this loader cannot promise it), a
//! member whose recipe is `status: planned` (which refuses to install, by
//! design), and a member that does not declare one of the engines the
//! collection claims for its game. Those are load-time refusals rather
//! than install-time failures on purpose. A collection that advertises a
//! set the user cannot install is worse than no collection, because the
//! panel would be recommending something the engine gate hides or the
//! runner refuses.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::capability::{is_known_engine, CapabilitySpec, EngineMatch, EngineScope};
use crate::capability_runner::CapabilityRegistry;

/// Categories a collection may declare.
///
/// The same vocabulary the module registry uses, because a collection is
/// listed next to capabilities and a "vr / graphics / qol / system" label
/// is what a reader already expects there. A typo is refused rather than
/// rendered: an uncategorised row is indistinguishable from a broken one.
pub const VALID_CATEGORIES: &[&str] = &["graphics", "qol", "system", "vr"];

/// Statuses a collection may declare.
pub const VALID_STATUSES: &[&str] = &["available", "planned"];

/// The shipped collections, compiled in.
///
/// The id is repeated next to the source so a panic can name the
/// collection that is wrong without a second scan of the YAML.
const BUILT_IN_COLLECTIONS: &[(&str, &str)] = &[
    (
        "vr-cyberpunk-2077-stack",
        include_str!("../collections/vr-cyberpunk-2077-stack.yaml"),
    ),
    (
        "vr-stalker-2-stack",
        include_str!("../collections/vr-stalker-2-stack.yaml"),
    ),
];

/// Where a loaded collection came from.
///
/// `Default` is `BuiltIn` so a `CollectionSpec` deserialises before the
/// loader has decided anything: the two fields below are not YAML, and a
/// missing one must not be an error.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CollectionOrigin {
    /// Shipped under `src-tauri/collections/`.
    #[default]
    BuiltIn,
    /// The user's `%LOCALAPPDATA%\Moddin\collections\`.
    Local,
}

/// One member of a collection.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionEntry {
    /// A registered capability id. This is the field the whole loader
    /// exists to protect: the install walks it one member at a time, and
    /// a typo here is an install that fails halfway.
    pub id: String,
    /// Whether the collection is incomplete without this member.
    /// Default: a member is required unless the author says otherwise.
    #[serde(default = "default_true")]
    pub required: bool,
    /// Why this member is in the set, shown in the install preview.
    #[serde(default)]
    pub note: Option<String>,
}

fn default_true() -> bool {
    true
}

/// A loaded collection.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSpec {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub status: String,
    /// The catalog game this collection was curated for, when it was
    /// curated for one. `None` means "not game specific", which the
    /// install path treats as "any game the members support".
    #[serde(default)]
    pub target_game: Option<String>,
    /// The engines the collection's game runs on. Every member must
    /// declare at least one of them: a collection that claims an engine
    /// its own members do not support is internally inconsistent, and the
    /// engine gate would hide part of it from the game it targets.
    #[serde(default)]
    pub required_engines: Vec<String>,
    pub capabilities: Vec<CollectionEntry>,
    /// Set by the loader, not by YAML. Where this spec came from, and
    /// which file it was parsed from, so a duplicate id can name both
    /// sides instead of only the loser.
    #[serde(skip)]
    pub origin: CollectionOrigin,
    #[serde(skip)]
    pub source: String,
}

/// Why a collection cannot be installed for the requested game.
///
/// The loader decides this with the same [`EngineScope`] `capability_list`
/// uses, so a collection and the capability cards cannot disagree about
/// whether a recipe fits a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CollectionBlockerKind {
    /// The collection is curated for another game.
    WrongGame,
    /// The engine gate hides at least one member from this game.
    EngineMismatch,
}

/// One reason a collection cannot install here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionBlocker {
    pub kind: CollectionBlockerKind,
    /// The member at fault, when the reason is about one member.
    pub capability_id: Option<String>,
    /// The engines that member does declare, for the explanation.
    pub supported_engines: Vec<String>,
    /// The game the collection was curated for, for `WrongGame`.
    pub target_game: Option<String>,
}

/// One member as the install preview shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPreviewEntry {
    pub id: String,
    /// The capability's own `displayName`, not the id, so the preview
    /// reads the way the game page reads.
    pub display_name: String,
    pub rationale: String,
    pub required: bool,
    /// The engine verdict for the requested game, exactly as
    /// `capability_list` reports it. Optional on the TypeScript side
    /// because a hand-built fixture may not care.
    pub engine_match: EngineMatch,
}

/// The install preview: the ordered members, and why the run cannot start.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPreview {
    /// Empty when the collection can install for the requested game.
    pub blocked: Vec<CollectionBlocker>,
    pub capabilities: Vec<CollectionPreviewEntry>,
}

/// What the panel and the AI assistant receive.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub category: String,
    pub status: String,
    pub target_game: Option<String>,
    pub required_engines: Vec<String>,
    pub capability_count: usize,
    pub required_count: usize,
    pub preset: CollectionPreview,
}

/// A collection file that did not load, and why.
///
/// Collected rather than logged straight to stderr so the rule "a local
/// file is skipped, not fatal" is testable: `eprintln!` is invisible to a
/// test, and a skip the test cannot see is a skip nobody checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionIssue {
    /// The file the issue came from: a path, or `built-in <id>`.
    pub source: String,
    pub message: String,
}

impl fmt::Display for CollectionIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.source, self.message)
    }
}

/// Everything the loader refused, in load order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectionLoadReport {
    pub issues: Vec<CollectionIssue>,
}

impl CollectionLoadReport {
    fn push(&mut self, source: &str, message: impl Into<String>) {
        self.issues.push(CollectionIssue {
            source: source.to_owned(),
            message: message.into(),
        });
    }
}

/// Every loaded collection, keyed by id.
#[derive(Debug, Default)]
pub struct CollectionRegistry {
    specs: HashMap<String, CollectionSpec>,
}

/// `%LOCALAPPDATA%\Moddin\collections\`, the local override directory.
///
/// Same place a user drops local recipes, one level over: the two files
/// are read by different loaders and neither knows about the other.
pub fn local_collections_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|dir| dir.join("Moddin").join("collections"))
}

impl CollectionRegistry {
    /// Built-ins, then the local override directory on top.
    ///
    /// A broken *built-in* panics, exactly as `insert_built_ins` does for
    /// a recipe: the YAML is compiled in, so a malformed one is a build
    /// bug, and shipping it would mean the panel shows a curated set that
    /// was never curated. `cargo test` parses every built-in through the
    /// same function, so this cannot fire from a build that ran its tests.
    pub fn load(capabilities: &CapabilityRegistry) -> (Self, CollectionLoadReport) {
        let mut registry = Self::default();
        let mut report = CollectionLoadReport::default();

        for (id, source) in BUILT_IN_COLLECTIONS {
            let label = format!("built-in collection {id}");
            let mut spec = parse(&label, source, capabilities)
                .unwrap_or_else(|message| panic!("moddin: {label} is invalid: {message}"));
            spec.origin = CollectionOrigin::BuiltIn;
            if registry.specs.insert(spec.id.clone(), spec).is_some() {
                panic!("moddin: duplicate built-in collection id {id}");
            }
        }

        if let Some(local_dir) = local_collections_dir() {
            registry.load_local_into(&local_dir, capabilities, &mut report);
        }

        (registry, report)
    }

    /// Same as [`Self::load`] with the local directory named explicitly.
    /// Test-only: nothing in the product needs to point the loader
    /// somewhere else, and an entry point that did would be a way to load
    /// untrusted YAML from anywhere.
    #[cfg(test)]
    fn load_with_local_dir(
        capabilities: &CapabilityRegistry,
        local_dir: &Path,
    ) -> (Self, CollectionLoadReport) {
        let mut registry = Self::default();
        let mut report = CollectionLoadReport::default();
        for (id, source) in BUILT_IN_COLLECTIONS {
            let label = format!("built-in collection {id}");
            let mut spec = parse(&label, source, capabilities)
                .unwrap_or_else(|message| panic!("moddin: {label} is invalid: {message}"));
            spec.origin = CollectionOrigin::BuiltIn;
            if registry.specs.insert(spec.id.clone(), spec).is_some() {
                panic!("moddin: duplicate built-in collection id {id}");
            }
        }
        registry.load_local_into(local_dir, capabilities, &mut report);
        (registry, report)
    }

    /// Read every YAML in `dir`. A file that cannot be read, cannot be
    /// parsed or does not validate is skipped with a reason; one bad file
    /// never takes the directory down with it.
    fn load_local_into(
        &mut self,
        dir: &Path,
        capabilities: &CapabilityRegistry,
        report: &mut CollectionLoadReport,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                report.push(
                    &dir.display().to_string(),
                    format!("could not be read: {error}"),
                );
                return;
            }
        };

        // Sorted so the "keeping the first" rule below is deterministic:
        // two files claiming one id is an authoring mistake either way,
        // but which of the two wins should not depend on the filesystem.
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();

        for path in paths {
            let label = path.display().to_string();
            let raw = match std::fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(error) => {
                    report.push(&label, format!("could not be read: {error}"));
                    continue;
                }
            };
            let mut spec = match parse(&label, &raw, capabilities) {
                Ok(spec) => spec,
                Err(message) => {
                    report.push(&label, format!("skipped: {message}"));
                    continue;
                }
            };
            spec.origin = CollectionOrigin::Local;
            spec.source = label.clone();

            match self.specs.get(&spec.id).map(|existing| existing.origin) {
                Some(CollectionOrigin::BuiltIn) => report.push(
                    &label,
                    format!(
                        "shadows the built-in collection `{}`; the local file is used",
                        spec.id
                    ),
                ),
                Some(CollectionOrigin::Local) => {
                    let previous = self.specs[&spec.id].source.clone();
                    report.push(
                        &label,
                        format!(
                            "refused: collection id `{}` is already loaded from {previous}; keeping the first",
                            spec.id
                        ),
                    );
                    continue;
                }
                None => {}
            }

            self.specs.insert(spec.id.clone(), spec);
        }
    }

    /// Test-only. Nothing in the product asks for one collection by id:
    /// the panel renders the whole list, and the install walks that list.
    /// A lookup that only tests call is not an entry point the module
    /// needs to expose.
    #[cfg(test)]
    pub fn get(&self, id: &str) -> Option<&CollectionSpec> {
        self.specs.get(id)
    }

    /// The list the panel renders, resolved against one game.
    ///
    /// `request` carries the selected game's catalog id and its
    /// `enginePreset`. The catalogue is two data sets that meet in Rust —
    /// the game's engine and a recipe's `supportedEngines` — and the same
    /// [`EngineScope`] resolution `capability_list` performs decides every
    /// verdict here, so the two lists cannot drift apart.
    pub fn summaries(
        &self,
        capabilities: &CapabilityRegistry,
        request: &CollectionListRequest,
    ) -> Vec<CollectionSummary> {
        let engine = request
            .engine
            .as_deref()
            .map(str::trim)
            .filter(|engine| !engine.is_empty());
        let game_id = request
            .game_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        let scope = EngineScope::resolve(engine, |engine| capabilities.declares_engine(engine));

        let mut summaries: Vec<CollectionSummary> = self
            .specs
            .values()
            .map(|spec| summary_for(spec, capabilities, game_id, scope))
            .collect();
        summaries.sort_by(|left, right| left.id.cmp(&right.id));
        summaries
    }
}

/// Build one summary: the members as the preview shows them, plus the
/// reasons the run cannot start.
fn summary_for(
    spec: &CollectionSpec,
    capabilities: &CapabilityRegistry,
    game_id: Option<&str>,
    scope: EngineScope<'_>,
) -> CollectionSummary {
    let mut blocked = Vec::new();
    if let (Some(game_id), Some(target)) = (game_id, spec.target_game.as_deref()) {
        if target != game_id {
            blocked.push(CollectionBlocker {
                kind: CollectionBlockerKind::WrongGame,
                capability_id: None,
                supported_engines: Vec::new(),
                target_game: Some(target.to_owned()),
            });
        }
    }

    let mut entries = Vec::with_capacity(spec.capabilities.len());
    for entry in &spec.capabilities {
        // Load-time validation already refused a collection naming a
        // capability the registry does not have, so this is an invariant
        // rather than an error path the caller can reach.
        let member: &CapabilitySpec = capabilities
            .get(&entry.id)
            .expect("collection members are validated against the registry at load time");
        let engine_match = member.engine_match(scope);
        if let EngineMatch::Mismatch { supported } = &engine_match {
            blocked.push(CollectionBlocker {
                kind: CollectionBlockerKind::EngineMismatch,
                capability_id: Some(entry.id.clone()),
                supported_engines: supported.clone(),
                target_game: None,
            });
        }
        entries.push(CollectionPreviewEntry {
            id: entry.id.clone(),
            display_name: member.display_name.clone(),
            rationale: entry.note.clone().unwrap_or_default(),
            required: entry.required,
            engine_match,
        });
    }

    CollectionSummary {
        id: spec.id.clone(),
        display_name: spec.display_name.clone(),
        description: spec.description.clone(),
        category: spec.category.clone(),
        status: spec.status.clone(),
        target_game: spec.target_game.clone(),
        required_engines: spec.required_engines.clone(),
        capability_count: spec.capabilities.len(),
        required_count: spec
            .capabilities
            .iter()
            .filter(|entry| entry.required)
            .count(),
        preset: CollectionPreview {
            blocked,
            capabilities: entries,
        },
    }
}

/// What `collection_list` asks about.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionListRequest {
    /// The catalog game the collections will be installed into.
    #[serde(default)]
    pub game_id: Option<String>,
    /// That game's `enginePreset`, when it declares one. The engine
    /// vocabulary and the gate live in Rust, so the caller passes the
    /// value the catalog holds and the loader applies the rule.
    #[serde(default)]
    pub engine: Option<String>,
}

/// Every loaded collection, resolved for the requested game.
///
/// The one command the collections UI calls. The registry is rebuilt per
/// call, as `capability_list` rebuilds its own: the cost is a few file
/// reads and it means a collection dropped into the local directory
/// appears on the next open without a restart command, so there is no
/// `collection_reload` to register.
#[tauri::command]
pub fn collection_list(request: CollectionListRequest) -> Vec<CollectionSummary> {
    let capabilities = CapabilityRegistry::load();
    let (collections, report) = CollectionRegistry::load(&capabilities);
    for issue in &report.issues {
        eprintln!("moddin: collection {issue}");
    }
    collections.summaries(&capabilities, &request)
}

/// Parse and validate one collection document.
///
/// `label` names the source in every message, so a refusal points at the
/// file to fix rather than at the loader.
fn parse(
    label: &str,
    raw: &str,
    capabilities: &CapabilityRegistry,
) -> Result<CollectionSpec, String> {
    let spec: CollectionSpec =
        serde_yaml::from_str(raw).map_err(|error| format!("could not be parsed: {error}"))?;

    if !is_collection_id(&spec.id) {
        return Err(format!(
            "id `{}` must be lowercase letters, digits and dashes",
            spec.id
        ));
    }
    if spec.display_name.trim().is_empty() {
        return Err("displayName is empty".to_owned());
    }
    if !VALID_CATEGORIES.contains(&spec.category.as_str()) {
        return Err(format!(
            "category `{}` is not one of {}",
            spec.category,
            VALID_CATEGORIES.join(", ")
        ));
    }
    if !VALID_STATUSES.contains(&spec.status.as_str()) {
        return Err(format!(
            "status `{}` is not one of {}",
            spec.status,
            VALID_STATUSES.join(", ")
        ));
    }
    if spec.capabilities.is_empty() {
        return Err(
            "capabilities is empty; a collection with no members installs nothing".to_owned(),
        );
    }

    for engine in &spec.required_engines {
        if !is_known_engine(engine) {
            return Err(format!(
                "requiredEngines names `{}`, which is not in {:?}",
                engine,
                crate::capability::KNOWN_ENGINES
            ));
        }
    }

    let mut seen: Vec<&str> = Vec::with_capacity(spec.capabilities.len());
    for entry in &spec.capabilities {
        if !seen.contains(&entry.id.as_str()) {
            seen.push(&entry.id);
        } else {
            return Err(format!("`{}` is listed twice", entry.id));
        }

        let member = match capabilities.get(&entry.id) {
            Some(member) => member,
            None if is_community_capability_id(&entry.id) => {
                // A community recipe is not a registered capability: it is
                // fetched from the signed catalogue and signature-checked
                // at install time, so this loader cannot say whether one is
                // signed. A collection is a promise about what a run will
                // do, and a member whose signature can only be checked
                // halfway through is a refusal the user learns about with
                // the earlier members already written. Refused here, with
                // the reason named, next to the two refusals above.
                return Err(format!(
                    "names community capability `{}`, which Moddin does not register: a community recipe is signature-checked when it installs, and a collection may not carry one",
                    entry.id
                ));
            }
            None => {
                return Err(format!(
                    "names capability `{}`, which is not a registered capability",
                    entry.id
                ));
            }
        };

        // A planned recipe refuses to install with a message instead of
        // reporting success while writing nothing. A collection that
        // lists one would show up in the panel as a set the user cannot
        // install — the branch's own two collections did exactly this by
        // naming `uevr`.
        if member.status != "available" {
            return Err(format!(
                "names capability `{}`, whose recipe is `{}` and refuses to install",
                entry.id, member.status
            ));
        }

        if !spec.required_engines.is_empty()
            && !spec
                .required_engines
                .iter()
                .any(|engine| member.supported_engines.contains(engine))
        {
            return Err(format!(
                "`{}` does not support any of the engines the collection claims ({})",
                entry.id,
                spec.required_engines.join(", ")
            ));
        }
    }

    let _ = label;
    let mut spec = spec;
    spec.source = label.to_owned();
    Ok(spec)
}

/// Ids are slugs because they become filenames: the built-in source is
/// `collections/<id>.yaml` and a local file is expected to follow the same
/// convention, so a collection named by the panel should look like a
/// collection a user could drop into their own directory.
fn is_collection_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

/// Whether a member id names a community recipe rather than a registered
/// capability.
///
/// The community repository publishes every capability as
/// `community-<name>`, and its submission guide asks for exactly that
/// (`mkdir capabilities/community-my-mod`, `id: community-my-mod`). That
/// prefix is the only signal available before the signed catalogue is
/// read: verifying a community entry is
/// [`crate::community_catalog`]'s job, at install time, with the keyring
/// check and the revocation list, and duplicating any of that here would
/// make the loader's answer depend on whether the user happened to open
/// the Community panel first. Fail-closed by design — an id that looks
/// like a community one is refused, whatever the catalogue says.
fn is_community_capability_id(id: &str) -> bool {
    id.starts_with("community-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(suffix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "moddin-collections-{suffix}-{}-{nanos}",
            std::process::id()
        ))
    }

    /// Built-in recipes only, so a developer's `%LOCALAPPDATA%` recipes
    /// cannot change what the assertions see.
    fn built_in_capabilities(label: &str) -> CapabilityRegistry {
        CapabilityRegistry::load_with_local_dir(temp_root(label))
    }

    fn write_collection(dir: &Path, file_name: &str, body: &str) {
        fs::create_dir_all(dir).expect("create temp dir");
        fs::write(dir.join(file_name), body).expect("write collection yaml");
    }

    fn collection_yaml(id: &str, members: &str) -> String {
        format!(
            "id: {id}\n\
             displayName: Local {id}\n\
             description: A collection written by a test.\n\
             category: vr\n\
             status: available\n\
             targetGame: stalker-2\n\
             requiredEngines:\n  - unreal5\n\
             capabilities:\n{members}"
        )
    }

    /// A collection that claims no engines at all, which is what a
    /// cross-engine set has to look like for the engine gate to judge it
    /// against the game it is offered for.
    fn collection_yaml_without_engines(id: &str, members: &str) -> String {
        format!(
            "id: {id}\n\
             displayName: Local {id}\n\
             description: A collection written by a test.\n\
             category: qol\n\
             status: available\n\
             targetGame: stalker-2\n\
             capabilities:\n{members}"
        )
    }

    #[test]
    fn every_built_in_collection_loads_and_names_real_capabilities() {
        let capabilities = built_in_capabilities("built-ins");
        let (registry, report) =
            CollectionRegistry::load_with_local_dir(&capabilities, &temp_root("built-ins-empty"));

        assert_eq!(report.issues, Vec::new(), "{report:?}");
        assert_eq!(registry.specs.len(), BUILT_IN_COLLECTIONS.len());

        let mut ids: Vec<&str> = registry.specs.keys().map(String::as_str).collect();
        ids.sort();
        assert_eq!(
            ids,
            vec!["vr-cyberpunk-2077-stack", "vr-stalker-2-stack"],
            "the shipped collections changed; this test names them one by one"
        );

        for spec in registry.specs.values() {
            assert!(!spec.capabilities.is_empty(), "{} has no members", spec.id);
            for entry in &spec.capabilities {
                let member = capabilities
                    .get(&entry.id)
                    .unwrap_or_else(|| panic!("{} names unknown {}", spec.id, entry.id));
                // The gate the install actually enforces.
                assert_eq!(
                    member.status, "available",
                    "{} names the planned recipe {}",
                    spec.id, entry.id
                );
            }
        }
    }

    /// The content test: every shipped collection must fit the game it
    /// targets, judged against the catalogue file that ships, not against
    /// a hard-coded engine. This is the check that keeps a collection from
    /// advertising a set the engine gate hides.
    #[test]
    fn every_built_in_collection_fits_the_game_it_targets() {
        let capabilities = built_in_capabilities("content");
        let (registry, report) =
            CollectionRegistry::load_with_local_dir(&capabilities, &temp_root("content-empty"));
        assert_eq!(report.issues, Vec::new(), "{report:?}");

        let games = shipped_games();
        assert!(!games.is_empty(), "no catalogue games were read");

        for spec in registry.specs.values() {
            let target = spec
                .target_game
                .as_deref()
                .unwrap_or_else(|| panic!("{} targets no game", spec.id));
            let game = games
                .iter()
                .find(|game| game.id == target)
                .unwrap_or_else(|| {
                    panic!(
                        "{} targets {}, which no catalogue game declares",
                        spec.id, target
                    )
                });
            let scope = EngineScope::resolve(game.engine_preset.as_deref(), |engine| {
                capabilities.declares_engine(engine)
            });

            for entry in &spec.capabilities {
                let member = capabilities.get(&entry.id).expect("validated at load time");
                let verdict = member.engine_match(scope);
                assert!(
                    verdict.is_eligible(),
                    "{}: the engine gate hides {} from {} ({:?} vs {:?})",
                    spec.id,
                    entry.id,
                    target,
                    game.engine_preset,
                    verdict
                );
            }
        }
    }

    #[test]
    fn a_local_collection_shadows_a_built_in_and_the_shadowing_is_reported() {
        let capabilities = built_in_capabilities("shadow");
        let local = temp_root("shadow-local");
        write_collection(
            &local,
            "vr-stalker-2-stack.yaml",
            &collection_yaml(
                "vr-stalker-2-stack",
                "  - id: optiscaler\n    required: true\n    note: Local note.\n",
            ),
        );

        let (registry, report) = CollectionRegistry::load_with_local_dir(&capabilities, &local);

        let spec = registry.get("vr-stalker-2-stack").expect("still loaded");
        assert_eq!(spec.origin, CollectionOrigin::Local);
        assert_eq!(spec.display_name, "Local vr-stalker-2-stack");
        // The shadowing is reported, not silent: one issue, naming the
        // file that did it.
        assert_eq!(report.issues.len(), 1, "{report:?}");
        assert!(
            report.issues[0]
                .message
                .contains("shadows the built-in collection"),
            "{report:?}"
        );
        assert!(report.issues[0].source.ends_with("vr-stalker-2-stack.yaml"));

        let _ = fs::remove_dir_all(&local);
    }

    #[test]
    fn a_malformed_local_collection_is_skipped_with_the_file_named() {
        let capabilities = built_in_capabilities("malformed");
        let local = temp_root("malformed-local");
        write_collection(&local, "bad.yaml", "id: bad\n  displayName: no\n");

        let (registry, report) = CollectionRegistry::load_with_local_dir(&capabilities, &local);

        // Skipped, not fatal: the built-ins are still there.
        assert_eq!(registry.specs.len(), BUILT_IN_COLLECTIONS.len());
        assert_eq!(report.issues.len(), 1, "{report:?}");
        assert!(
            report.issues[0].source.ends_with("bad.yaml"),
            "the message must name the file: {report:?}"
        );
        assert!(
            report.issues[0].message.contains("could not be parsed"),
            "{report:?}"
        );

        let _ = fs::remove_dir_all(&local);
    }

    #[test]
    fn a_duplicate_collection_id_is_refused_and_both_files_are_named() {
        let capabilities = built_in_capabilities("duplicate");
        let local = temp_root("duplicate-local");
        write_collection(
            &local,
            "a-pair.yaml",
            &collection_yaml("local-pair", "  - id: optiscaler\n    required: true\n"),
        );
        write_collection(
            &local,
            "b-pair.yaml",
            &collection_yaml("local-pair", "  - id: ofxr-bridge\n    required: true\n"),
        );

        let (registry, report) = CollectionRegistry::load_with_local_dir(&capabilities, &local);

        // The first one wins, and the second is refused rather than
        // silently replacing it.
        let spec = registry.get("local-pair").expect("first file kept");
        assert_eq!(spec.capabilities[0].id, "optiscaler");
        assert_eq!(report.issues.len(), 1, "{report:?}");
        assert!(report.issues[0].message.contains("refused"), "{report:?}");
        assert!(
            report.issues[0].source.ends_with("b-pair.yaml"),
            "{report:?}"
        );
        assert!(
            report.issues[0].message.contains("a-pair.yaml"),
            "{report:?}"
        );

        let _ = fs::remove_dir_all(&local);
    }

    #[test]
    fn a_collection_naming_an_unregistered_capability_is_refused_at_load_time() {
        let capabilities = built_in_capabilities("unregistered");
        let local = temp_root("unregistered-local");
        write_collection(
            &local,
            "typo.yaml",
            &collection_yaml("has-a-typo", "  - id: ofxr-brige\n    required: true\n"),
        );

        let (registry, report) = CollectionRegistry::load_with_local_dir(&capabilities, &local);

        // The point of the loader: this is refused before the panel can
        // offer it, not discovered halfway through an install.
        assert!(registry.get("has-a-typo").is_none());
        assert_eq!(report.issues.len(), 1, "{report:?}");
        assert!(
            report.issues[0]
                .message
                .contains("`ofxr-brige`, which is not a registered capability"),
            "{report:?}"
        );
        assert!(report.issues[0].source.ends_with("typo.yaml"));

        let _ = fs::remove_dir_all(&local);
    }

    #[test]
    fn a_collection_naming_a_planned_recipe_is_refused() {
        let capabilities = built_in_capabilities("planned");
        let local = temp_root("planned-local");
        write_collection(
            &local,
            "uevr-set.yaml",
            &collection_yaml("uevr-set", "  - id: uevr\n    required: true\n"),
        );

        let (registry, report) = CollectionRegistry::load_with_local_dir(&capabilities, &local);

        // `uevr` is `status: planned` and refuses to install, so a
        // collection naming it is exactly the set the user cannot install.
        assert!(registry.get("uevr-set").is_none());
        assert_eq!(report.issues.len(), 1, "{report:?}");
        assert!(
            report.issues[0]
                .message
                .contains("`uevr`, whose recipe is `planned` and refuses to install"),
            "{report:?}"
        );

        let _ = fs::remove_dir_all(&local);
    }

    #[test]
    fn a_collection_naming_an_unsigned_community_member_is_refused_at_load_time() {
        let capabilities = built_in_capabilities("community");
        let local = temp_root("community-local");
        write_collection(
            &local,
            "with-community.yaml",
            &collection_yaml(
                "with-community",
                "  - id: optiscaler\n    required: true\n  - id: community-fps-unlocker\n    required: false\n",
            ),
        );

        let (registry, report) = CollectionRegistry::load_with_local_dir(&capabilities, &local);

        // `community-fps-unlocker` is the shipped community entry: it is
        // `status: planned` and unsigned, and the only catalogue entry
        // there is. A collection run installs `acceptUnsigned: false`, so
        // carrying it would be a refusal discovered on the second member
        // with the first one already written. Refused at load, and the
        // message says *why* rather than calling it a typo.
        assert!(registry.get("with-community").is_none());
        assert_eq!(report.issues.len(), 1, "{report:?}");
        assert!(
            report.issues[0]
                .message
                .contains("names community capability `community-fps-unlocker`"),
            "{report:?}"
        );
        assert!(
            report.issues[0]
                .message
                .contains("a community recipe is signature-checked when it installs"),
            "{report:?}"
        );
        // Not the generic "not a registered capability" wording: the
        // reason is that its signature is unknowable here, and a message
        // that says otherwise sends the author looking for a typo.
        assert!(
            !report.issues[0]
                .message
                .contains("which is not a registered capability"),
            "{report:?}"
        );

        let _ = fs::remove_dir_all(&local);
    }

    #[test]
    fn a_member_that_does_not_declare_the_claimed_engines_is_refused() {
        let capabilities = built_in_capabilities("engines");
        let local = temp_root("engines-local");
        // `bepinex` declares unity, and the collection claims unreal5.
        write_collection(
            &local,
            "mismatch.yaml",
            &collection_yaml("wrong-engine-set", "  - id: bepinex\n    required: true\n"),
        );

        let (registry, report) = CollectionRegistry::load_with_local_dir(&capabilities, &local);

        assert!(registry.get("wrong-engine-set").is_none());
        assert_eq!(report.issues.len(), 1, "{report:?}");
        assert!(
            report.issues[0]
                .message
                .contains("does not support any of the engines the collection claims"),
            "{report:?}"
        );

        let _ = fs::remove_dir_all(&local);
    }

    fn request(game_id: Option<&str>, engine: Option<&str>) -> CollectionListRequest {
        CollectionListRequest {
            game_id: game_id.map(str::to_owned),
            engine: engine.map(str::to_owned),
        }
    }

    #[test]
    fn a_collection_for_another_game_is_reported_in_the_preview() {
        let capabilities = built_in_capabilities("wrong-game");
        let (registry, _) =
            CollectionRegistry::load_with_local_dir(&capabilities, &temp_root("wrong-game-empty"));

        let summaries = registry.summaries(&capabilities, &request(Some("elden-ring"), None));
        let cyberpunk = summaries
            .iter()
            .find(|summary| summary.id == "vr-cyberpunk-2077-stack")
            .expect("shipped collection listed");
        assert_eq!(cyberpunk.preset.capabilities.len(), 2);
        assert_eq!(
            cyberpunk.preset.blocked,
            vec![CollectionBlocker {
                kind: CollectionBlockerKind::WrongGame,
                capability_id: None,
                supported_engines: Vec::new(),
                target_game: Some("cyberpunk-2077".to_owned()),
            }]
        );
    }

    #[test]
    fn the_engine_verdict_rides_with_every_member() {
        let capabilities = built_in_capabilities("verdict");
        let (registry, _) =
            CollectionRegistry::load_with_local_dir(&capabilities, &temp_root("verdict-empty"));

        // Cyberpunk is redengine; both members declare it.
        let summaries = registry.summaries(
            &capabilities,
            &request(Some("cyberpunk-2077"), Some("redengine")),
        );
        let cyberpunk = summaries
            .iter()
            .find(|summary| summary.id == "vr-cyberpunk-2077-stack")
            .expect("listed");
        assert!(
            cyberpunk.preset.blocked.is_empty(),
            "{:?}",
            cyberpunk.preset
        );
        for entry in &cyberpunk.preset.capabilities {
            assert_eq!(
                entry.engine_match,
                EngineMatch::Supported,
                "{} for the game it targets",
                entry.id
            );
        }

        // The same collection viewed from an Unreal 5 game: the gate
        // hides `reshade`-only members, and the preview says so instead
        // of the install discovering it at step one.
        let summaries =
            registry.summaries(&capabilities, &request(Some("stalker-2"), Some("unreal5")));
        let cyberpunk = summaries
            .iter()
            .find(|summary| summary.id == "vr-cyberpunk-2077-stack")
            .expect("listed");
        assert_eq!(
            cyberpunk.preset.blocked[0].kind,
            CollectionBlockerKind::WrongGame,
            "the target game still blocks first"
        );

        // A local collection whose member the gate hides, so the mismatch
        // itself is exercised.
        let local = temp_root("verdict-local");
        write_collection(
            &local,
            "unity-only.yaml",
            &collection_yaml_without_engines("unity-only", "  - id: bepinex\n    required: true\n"),
        );
        let (registry, report) = CollectionRegistry::load_with_local_dir(&capabilities, &local);
        assert_eq!(report.issues, Vec::new(), "{report:?}");
        let summaries =
            registry.summaries(&capabilities, &request(Some("stalker-2"), Some("unreal5")));
        let unity_only = summaries
            .iter()
            .find(|summary| summary.id == "unity-only")
            .expect("listed");
        assert_eq!(
            unity_only.preset.blocked[0].kind,
            CollectionBlockerKind::EngineMismatch
        );
        assert_eq!(
            unity_only.preset.blocked[0].capability_id.as_deref(),
            Some("bepinex")
        );
        assert_eq!(
            unity_only.preset.blocked[0].supported_engines,
            vec!["unity".to_owned()]
        );

        let _ = fs::remove_dir_all(&local);
    }

    #[test]
    fn a_built_in_collection_naming_itself_keeps_the_config_from_the_yaml() {
        let capabilities = built_in_capabilities("no-local");
        let (registry, report) = CollectionRegistry::load_with_local_dir(
            &capabilities,
            Path::new("this-directory-does-not-exist"),
        );

        // A missing local directory is not an issue. The user may never
        // write one.
        assert_eq!(report.issues, Vec::new(), "{report:?}");
        let summaries = registry.summaries(
            &capabilities,
            &request(Some("cyberpunk-2077"), Some("redengine")),
        );
        assert_eq!(summaries.len(), BUILT_IN_COLLECTIONS.len());
        for summary in &summaries {
            assert!(!summary.preset.capabilities.is_empty());
            for entry in &summary.preset.capabilities {
                // The preview shows the capability's own name, not its id.
                assert_ne!(entry.display_name, entry.id, "{summary:?}");
            }
        }

        // The collection for the game that was asked about is clean, and
        // the one curated for another game says so instead of installing.
        let cyberpunk = summaries
            .iter()
            .find(|summary| summary.id == "vr-cyberpunk-2077-stack")
            .expect("listed");
        assert!(cyberpunk.preset.blocked.is_empty(), "{cyberpunk:?}");
        let stalker = summaries
            .iter()
            .find(|summary| summary.id == "vr-stalker-2-stack")
            .expect("listed");
        assert_eq!(
            stalker.preset.blocked[0].kind,
            CollectionBlockerKind::WrongGame
        );
    }

    /// A shipped catalogue game, read from the YAML that ships.
    #[derive(Debug, Deserialize)]
    struct ShippedGame {
        id: String,
        #[serde(rename = "enginePreset")]
        engine_preset: Option<String>,
    }

    fn shipped_games() -> Vec<ShippedGame> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("src")
            .join("catalog")
            .join("games");
        let mut games: Vec<ShippedGame> = std::fs::read_dir(&dir)
            .expect("read the catalogue games directory")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("yaml"))
            .map(|path| {
                let raw = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                // `dawnwalker.yaml` ships a UTF-8 BOM and `serde_yaml`
                // rejects one ("YAML containing more than one document"),
                // so it is stripped here. Nothing in the app parses these
                // files from Rust — the catalogue loader is the frontend's
                // — so the BOM costs this test a line and the app
                // nothing; the file is worth reporting separately.
                let raw = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
                serde_yaml::from_str(raw)
                    .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
            })
            .collect();
        games.sort_by(|left, right| left.id.cmp(&right.id));
        games
    }
}
