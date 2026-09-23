//! Built-in and user-authored "mod collections" — a single named
//! bundle of capabilities that get installed together, in order, with
//! rollback support.
//!
//! ## Why collections
//!
//! Some game / mod setups only work when several capabilities are
//! installed together in a specific order (VR + frame-gen + OpenXR
//! runtime, for example). Asking the user to apply them one by one,
//! in the right order, with the right config, is where the
//! non-modder gives up. A collection is the answer: pick one,
//! click install, the rest happens behind Moddin's normal preview /
//! transaction / rollback machinery.
//!
//! ## Where collections live
//!
//! - **Built-in** — compiled into the binary under
//!   `src-tauri/collections/<id>.yaml`. Curated by the Moddin
//!   maintainers for the most common setups.
//! - **Local** — dropped by the user (or by the AI authoring flow)
//!   under `%LOCALAPPDATA%/Moddin/collections/<id>.yaml`. Origin is
//!   `Local` so the UI can badge it as user-editable.
//! - **Community** — future: a `collections` array in the signed
//!   community catalog, served the same way as community capabilities.
//!   The struct already supports it; the wiring is intentionally not
//!   in this iteration.
//!
//! ## Schema
//!
//! Mirrors `CapabilitySpec` for the top-level fields the schema
//! validator already understands (id, displayName, category, status),
//! then adds:
//!
//! - `targetGame` — Moddin game id the collection is meant for.
//! - `requiredEngines` — optional engine allow-list (Unreal5, Redengine…).
//! - `capabilities` — ordered list of capability ids to install.
//!   Each entry can be marked `required: false` to make the install
//!   best-effort (a failure skips and continues) or `required: true`
//!   to halt the whole collection.

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Valid `category` strings. Mirrors `CapabilitySpec.category` so a
/// collection shows up alongside the capabilities it groups.
pub const VALID_CATEGORIES: &[&str] = &["vr", "graphics", "qol", "system"];

/// Valid `status` strings.
pub const VALID_STATUSES: &[&str] = &["available", "planned"];

/// Provenance of a collection — drives the UI badge and the
/// community-signature path.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CollectionOrigin {
    BuiltIn,
    Local,
    Community,
}

impl Default for CollectionOrigin {
    fn default() -> Self {
        CollectionOrigin::BuiltIn
    }
}

/// One capability inside a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionEntry {
    /// Capability id, e.g. `"ofxr-bridge"`. Must exist in the
    /// capability registry when install runs.
    pub id: String,
    /// When `true` (default), a failure aborts the collection and
    /// surfaces the mid-install prompt. When `false`, the failure is
    /// skipped and the next entry runs.
    #[serde(default = "default_required")]
    pub required: bool,
    /// Optional free-form note shown in the UI ("best for FPS-heavy
    /// games"). Not interpreted by the runner.
    #[serde(default)]
    pub note: Option<String>,
    /// Optional config override for this capability. When set, the
    /// collection skips the per-capability config dialog and uses
    /// these values directly. Reserved for future use — the runner
    /// does not yet consume it.
    #[serde(default)]
    pub config: Option<BTreeMap<String, JsonValue>>,
}

fn default_required() -> bool {
    true
}

/// Top-level collection definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSpec {
    pub id: String,
    pub display_name: String,
    /// Short explanation shown in the install dialog.
    #[serde(default)]
    pub description: String,
    pub category: String,
    pub status: String,
    /// Moddin game id the collection targets, e.g. `"cyberpunk-2077"`.
    /// When set, the UI greys out the collection for other games.
    #[serde(default)]
    pub target_game: Option<String>,
    /// Optional engine allow-list (Unreal5, Redengine, RE Engine…).
    /// Mirrors `CapabilitySpec.supportedEngines`.
    #[serde(default)]
    pub required_engines: Vec<String>,
    /// Ordered list of capabilities. Order matters: install runs
    /// top-to-bottom.
    pub capabilities: Vec<CollectionEntry>,
    /// Free-form notes shown above the capability list in the UI
    /// (e.g. "Close the game and any overlays before installing").
    #[serde(default)]
    pub safety_notes: Vec<String>,
    /// Set by the registry after load, not parsed from YAML.
    #[serde(default, skip_deserializing)]
    pub origin: CollectionOrigin,
}

/// Lightweight summary used by the UI to render the collection list.
/// Mirrors `crate::capability_runner::CapabilitySummary`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub status: String,
    pub origin: CollectionOrigin,
    pub target_game: Option<String>,
    pub capability_count: usize,
    pub required_count: usize,
    pub description: String,
}

/// In-memory registry of every collection Moddin knows about: the
/// built-ins compiled into the binary plus everything the user dropped
/// under `%LOCALAPPDATA%/Moddin/collections/`.
#[derive(Debug, Default)]
pub struct CollectionRegistry {
    specs: HashMap<String, CollectionSpec>,
}

impl CollectionRegistry {
    /// Load built-in collections (compiled in) plus any local YAML
    /// files dropped under `<local_dir>`. Malformed files are logged
    /// and skipped — they do not panic the runner.
    pub fn load(local_dir: Option<&Path>) -> Self {
        let mut specs = HashMap::new();

        for raw in BUILT_IN_COLLECTIONS {
            match serde_yaml::from_str::<CollectionSpec>(raw) {
                Ok(mut spec) => {
                    spec.origin = CollectionOrigin::BuiltIn;
                    if specs.contains_key(&spec.id) {
                        eprintln!(
                            "[moddin] duplicate built-in collection id '{}' — keeping the first one",
                            spec.id
                        );
                        continue;
                    }
                    specs.insert(spec.id.clone(), spec);
                }
                Err(error) => {
                    eprintln!("[moddin] could not parse built-in collection YAML: {error}");
                }
            }
        }

        if let Some(dir) = local_dir {
            load_local_into(dir, &mut specs);
        }

        Self { specs }
    }

    pub fn reload_local<P: AsRef<Path>>(&mut self, local_dir: P) {
        let local_dir = local_dir.as_ref();
        let mut keep: HashMap<String, CollectionSpec> = HashMap::new();
        for (id, spec) in self.specs.drain() {
            if spec.origin != CollectionOrigin::Local {
                keep.insert(id, spec);
            }
        }
        load_local_into(local_dir, &mut keep);
        self.specs.extend(keep);
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.specs.keys().map(|s| s.as_str())
    }

    pub fn get(&self, id: &str) -> Option<&CollectionSpec> {
        self.specs.get(id)
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Lightweight summaries for the UI list view.
    pub fn summaries(&self) -> Vec<CollectionSummary> {
        let mut out: Vec<CollectionSummary> = self
            .specs
            .values()
            .map(|spec| CollectionSummary {
                id: spec.id.clone(),
                display_name: spec.display_name.clone(),
                category: spec.category.clone(),
                status: spec.status.clone(),
                origin: spec.origin,
                target_game: spec.target_game.clone(),
                capability_count: spec.capabilities.len(),
                required_count: spec
                    .capabilities
                    .iter()
                    .filter(|c| c.required)
                    .count(),
                description: spec.description.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
        out
    }
}

fn load_local_into(dir: &Path, into: &mut HashMap<String, CollectionSpec>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            eprintln!(
                "[moddin] could not read collections directory {}: {error}",
                dir.display()
            );
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".yaml") && !name.to_ascii_lowercase().ends_with(".yml") {
            continue;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(error) => {
                eprintln!("[moddin] could not read {}: {error}", path.display());
                continue;
            }
        };
        match serde_yaml::from_str::<CollectionSpec>(&raw) {
            Ok(mut spec) => {
                spec.origin = CollectionOrigin::Local;
                if let Some(existing) = into.get(&spec.id) {
                    if existing.origin == CollectionOrigin::BuiltIn {
                        eprintln!(
                            "[moddin] local collection '{}' shadows a built-in; rename the file to avoid the conflict",
                            spec.id
                        );
                    }
                }
                into.insert(spec.id.clone(), spec);
            }
            Err(error) => {
                eprintln!("[moddin] malformed collection {}: {error}", path.display());
            }
        }
    }
}

/// Built-in collections compiled into the binary. Add new entries
/// here — keep the YAML inline so there is no second file to ship.
const BUILT_IN_COLLECTIONS: &[&str] = &[
    VR_CYBERPUNK_ESSENTIAL_YAML,
    VR_ELDEN_RING_STARTER_YAML,
];

const VR_CYBERPUNK_ESSENTIAL_YAML: &str = r#"id: vr-cyberpunk-essential
displayName: VR Cyberpunk — Essencial
description: Pacote mínimo pra rodar Cyberpunk 2077 em VR com boa qualidade.
category: vr
status: available
targetGame: cyberpunk-2077
requiredEngines:
  - redengine
safetyNotes:
  - Feche o jogo (e qualquer overlay) antes de instalar.
  - O Moddin não executa instaladores de terceiros automaticamente.
capabilities:
  - id: optiscaler
    required: true
    note: Upscaling com frame-gen é o coração do VR confortável em Cyberpunk.
  - id: openxr-helpers
    required: true
    note: Configura o OpenXR runtime por jogo, sem mexer no sistema global.
  - id: ofxr-bridge
    required: true
    note: Adiciona frame-gen compatível com OpenXR.
  - id: cheeky-foveated-dlss
    required: false
    note: Opcional — foveated rendering para GPUs mais fracas.
"#;

const VR_ELDEN_RING_STARTER_YAML: &str = r#"id: vr-elden-ring-starter
displayName: VR Elden Ring — Starter
description: Pacote mínimo pra começar a testar Elden Ring em VR.
category: vr
status: available
targetGame: elden-ring
safetyNotes:
  - Elden Ring em VR é experimental — não há release oficial testado.
  - Feche o jogo e qualquer overlay antes de instalar.
capabilities:
  - id: openxr-helpers
    required: true
    note: Runtime OpenXR por jogo.
  - id: uevr
    required: true
    note: Engine-aware injector para Unreal genéricos.
  - id: optiscaler
    required: false
    note: Opcional — ajuda a manter FPS estável.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_built_in_collections() {
        let registry = CollectionRegistry::load(None);
        assert!(!registry.is_empty(), "expected at least one built-in");
        assert!(registry.get("vr-cyberpunk-essential").is_some());
        assert!(registry.get("vr-elden-ring-starter").is_some());
    }

    #[test]
    fn built_in_collections_have_required_capabilities() {
        let registry = CollectionRegistry::load(None);
        let cyber = registry.get("vr-cyberpunk-essential").expect("present");
        assert!(cyber.capabilities.iter().any(|c| c.id == "ofxr-bridge"));
        let elden = registry.get("vr-elden-ring-starter").expect("present");
        assert!(elden.capabilities.iter().any(|c| c.id == "uevr"));
    }

    #[test]
    fn summaries_include_origin() {
        let registry = CollectionRegistry::load(None);
        let summaries = registry.summaries();
        for summary in &summaries {
            assert_eq!(summary.origin, CollectionOrigin::BuiltIn);
            assert!(!summary.display_name.is_empty());
        }
    }
}
