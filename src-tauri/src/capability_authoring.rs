//! Capability authoring commands backing the in-app AI assistant flow
//! ("ask AI to author a Moddin capability").
//!
//! The AI in the loop (ChatGPT / Claude / Gemini, via the copied prompt)
//! returns a YAML recipe; the frontend pastes it back and these commands
//! validate it against the real [`CapabilitySpec`] schema, render a
//! human-readable install plan, and persist it into the user's local
//! capability directory (`%LOCALAPPDATA%\Moddin\capabilities\`) where the
//! registry picks it up on the next `capability_list` call.
//!
//! Nothing here invents schema rules: parsing reuses `CapabilitySpec`,
//! and step / check kinds are checked against the same `known_kinds`
//! tables the runner dispatches on.

use crate::capability::{CapabilitySpec, SpecOrigin};
use crate::capability_runner::{local_capabilities_dir, CapabilityRegistry};
use serde::{Deserialize, Serialize};
use std::path::Path;

const VALID_CATEGORIES: &[&str] = &["vr", "graphics", "qol", "system"];
const VALID_STATUSES: &[&str] = &["available", "planned"];
const VALID_CONFIG_TYPES: &[&str] =
    &["string", "number", "boolean", "url", "sha256", "path", "enum"];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorYamlRequest {
    pub yaml: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorSpecSummary {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub status: String,
    pub supported_engines: Vec<String>,
    pub config_fields: Vec<String>,
    pub install_steps: usize,
    pub uninstall_steps: usize,
    pub verify_checks: usize,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub spec: Option<AuthorSpecSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorPreviewResult {
    pub ok: bool,
    pub plan: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorSaveRequest {
    pub yaml: String,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorSaveResult {
    pub ok: bool,
    pub id: String,
    pub path: String,
    pub overwrote: bool,
    pub errors: Vec<String>,
    pub exists: bool,
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

/// Parse the YAML and run the semantic checks the serde schema cannot
/// express (id shape, enum values, known step / check kinds). Returns
/// every problem at once so the AI can fix them all in one reply.
fn parse_and_check(yaml: &str) -> Result<CapabilitySpec, Vec<String>> {
    let spec: CapabilitySpec = match serde_yaml::from_str(yaml) {
        Ok(spec) => spec,
        Err(error) => return Err(vec![format!("Not valid YAML for a Moddin mod: {error}")]),
    };

    let mut errors = Vec::new();

    if !is_valid_id(&spec.id) {
        errors.push(format!(
            "id \"{}\" is invalid. Use lowercase letters, digits and dashes (e.g. \"fps-unlocker\").",
            spec.id
        ));
    }
    if !VALID_CATEGORIES.contains(&spec.category.as_str()) {
        errors.push(format!(
            "category \"{}\" is not valid. Use one of: {}.",
            spec.category,
            VALID_CATEGORIES.join(", ")
        ));
    }
    if !VALID_STATUSES.contains(&spec.status.as_str()) {
        errors.push(format!(
            "status \"{}\" is not valid. Use one of: {}.",
            spec.status,
            VALID_STATUSES.join(", ")
        ));
    }

    let known_steps = crate::builtin_steps::known_kinds();
    for (index, step) in spec.install.iter().enumerate() {
        if !known_steps.contains(&step.kind.as_str()) {
            errors.push(format!(
                "install step {} uses unknown kind \"{}\". Known kinds: {}.",
                index + 1,
                step.kind,
                known_steps.join(", ")
            ));
        }
    }
    for (index, step) in spec.uninstall.iter().enumerate() {
        if !known_steps.contains(&step.kind.as_str()) {
            errors.push(format!(
                "uninstall step {} uses unknown kind \"{}\". Known kinds: {}.",
                index + 1,
                step.kind,
                known_steps.join(", ")
            ));
        }
    }

    let known_checks = crate::builtin_checks::known_kinds();
    for check in spec.checks.iter().chain(spec.verify.iter()) {
        if !known_checks.contains(&check.kind.as_str()) {
            errors.push(format!(
                "check \"{}\" uses unknown kind \"{}\". Known kinds: {}.",
                check.id,
                check.kind,
                known_checks.join(", ")
            ));
        }
    }

    for field in &spec.config_schema {
        if !VALID_CONFIG_TYPES.contains(&field.field_type.as_str()) {
            errors.push(format!(
                "config field \"{}\" has unknown type \"{}\". Known types: {}.",
                field.name,
                field.field_type,
                VALID_CONFIG_TYPES.join(", ")
            ));
        }
        if field.field_type == "enum" && field.enum_values.is_empty() {
            errors.push(format!(
                "config field \"{}\" is an enum but lists no enumValues.",
                field.name
            ));
        }
    }

    if spec.install.is_empty() && spec.checks.is_empty() && spec.verify.is_empty() {
        errors.push(
            "The mod declares no install steps and no checks — there is nothing for Moddin to do."
                .to_owned(),
        );
    }

    if errors.is_empty() {
        Ok(spec)
    } else {
        Err(errors)
    }
}

fn summarize(spec: &CapabilitySpec) -> AuthorSpecSummary {
    AuthorSpecSummary {
        id: spec.id.clone(),
        display_name: spec.display_name.clone(),
        category: spec.category.clone(),
        status: spec.status.clone(),
        supported_engines: spec.supported_engines.clone(),
        config_fields: spec
            .config_schema
            .iter()
            .map(|field| field.name.clone())
            .collect(),
        install_steps: spec.install.len(),
        uninstall_steps: spec.uninstall.len(),
        verify_checks: spec.checks.len() + spec.verify.len(),
        safety_notes: spec.safety_notes.clone(),
    }
}

/// Render the dry-run plan the UI shows before saving. Plain sentences —
/// this text is for players, not engineers.
fn render_plan(spec: &CapabilitySpec) -> String {
    let mut lines = vec![format!(
        "{} ({}) — category {}, status {}.",
        spec.display_name, spec.id, spec.category, spec.status
    )];

    if !spec.install.is_empty() {
        lines.push(String::new());
        lines.push(format!("Install — {} steps:", spec.install.len()));
        for (index, step) in spec.install.iter().enumerate() {
            let detail = step
                .description
                .clone()
                .unwrap_or_else(|| format!("runs the \"{}\" step", step.kind));
            lines.push(format!("  {}. {}", index + 1, detail));
        }
    }

    lines.push(String::new());
    if spec.uninstall.is_empty() {
        lines.push("Uninstall: rolls back the recorded install transaction.".to_owned());
    } else {
        lines.push(format!(
            "Uninstall — {} explicit steps.",
            spec.uninstall.len()
        ));
    }

    let check_count = spec.checks.len() + spec.verify.len();
    if check_count > 0 {
        lines.push(format!(
            "Verification: {} check{} ({} after install).",
            check_count,
            if check_count == 1 { "" } else { "s" },
            spec.verify.len()
        ));
    }

    if !spec.config_schema.is_empty() {
        let fields = spec
            .config_schema
            .iter()
            .map(|field| {
                let required = if field.required { "required" } else { "optional" };
                format!("{} ({}, {})", field.name, field.field_type, required)
            })
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("Moddin will ask for: {fields}."));
    }

    if !spec.safety_notes.is_empty() {
        lines.push(String::new());
        lines.push("Safety notes:".to_owned());
        for note in &spec.safety_notes {
            lines.push(format!("  - {note}"));
        }
    }

    lines.join("\n")
}

#[tauri::command]
pub fn validate_capability_yaml(request: AuthorYamlRequest) -> AuthorValidationResult {
    match parse_and_check(&request.yaml) {
        Ok(spec) => AuthorValidationResult {
            ok: true,
            errors: Vec::new(),
            spec: Some(summarize(&spec)),
        },
        Err(errors) => AuthorValidationResult {
            ok: false,
            errors,
            spec: None,
        },
    }
}

#[tauri::command]
pub fn preview_capability_plan(request: AuthorYamlRequest) -> AuthorPreviewResult {
    match parse_and_check(&request.yaml) {
        Ok(spec) => AuthorPreviewResult {
            ok: true,
            plan: render_plan(&spec),
        },
        Err(errors) => AuthorPreviewResult {
            ok: false,
            plan: errors.join("\n"),
        },
    }
}

#[tauri::command]
pub fn save_capability_yaml(request: AuthorSaveRequest) -> AuthorSaveResult {
    let Some(dir) = local_capabilities_dir() else {
        return AuthorSaveResult {
            ok: false,
            id: String::new(),
            path: String::new(),
            overwrote: false,
            errors: vec!["The local capability folder is not available on this system.".to_owned()],
            exists: false,
        };
    };
    save_capability_to_dir(&dir, &request.yaml, request.overwrite)
}

/// Persist a validated capability into `dir`. Extracted from the Tauri
/// command so tests can point it at a temp directory without touching
/// `LOCALAPPDATA`.
fn save_capability_to_dir(dir: &Path, yaml: &str, overwrite: bool) -> AuthorSaveResult {
    let spec = match parse_and_check(yaml) {
        Ok(spec) => spec,
        Err(errors) => {
            return AuthorSaveResult {
                ok: false,
                id: String::new(),
                path: String::new(),
                overwrote: false,
                errors,
                exists: false,
            }
        }
    };

    // Local YAMLs override built-ins by design, but silently shadowing a
    // shipped mod from an AI draft is a support trap — refuse and ask for
    // another id.
    let registry = CapabilityRegistry::load();
    if let Some(existing) = registry.get(&spec.id) {
        if existing.origin == SpecOrigin::BuiltIn {
            return AuthorSaveResult {
                ok: false,
                id: spec.id.clone(),
                path: String::new(),
                overwrote: false,
                errors: vec![format!(
                    "\"{}\" is built into Moddin. Save your mod under a different id.",
                    spec.id
                )],
                exists: false,
            };
        }
    }

    let path = dir.join(format!("{}.yaml", spec.id));
    let exists = path.is_file();
    if exists && !overwrite {
        return AuthorSaveResult {
            ok: false,
            id: spec.id.clone(),
            path: path.display().to_string(),
            overwrote: false,
            errors: vec![format!(
                "\"{}\" is already saved locally. Confirm replacing it.",
                spec.id
            )],
            exists: true,
        };
    }

    if let Err(error) = std::fs::create_dir_all(dir) {
        return AuthorSaveResult {
            ok: false,
            id: spec.id.clone(),
            path: String::new(),
            overwrote: false,
            errors: vec![format!("Could not create the capability folder: {error}")],
            exists,
        };
    }

    // Keep a one-deep backup so a bad overwrite never destroys the last
    // working version of the mod.
    if exists {
        let backup = dir.join(format!("{}.yaml.bak", spec.id));
        if let Err(error) = std::fs::copy(&path, &backup) {
            return AuthorSaveResult {
                ok: false,
                id: spec.id.clone(),
                path: path.display().to_string(),
                overwrote: false,
                errors: vec![format!("Could not back up the existing file: {error}")],
                exists: true,
            };
        }
    }

    // Atomic write: a crash mid-save must never leave a truncated YAML
    // behind for the registry to trip over.
    let tmp = dir.join(format!("{}.yaml.tmp", spec.id));
    if let Err(error) = std::fs::write(&tmp, yaml) {
        return AuthorSaveResult {
            ok: false,
            id: spec.id.clone(),
            path: path.display().to_string(),
            overwrote: false,
            errors: vec![format!("Could not write the file: {error}")],
            exists,
        };
    }
    if let Err(error) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return AuthorSaveResult {
            ok: false,
            id: spec.id.clone(),
            path: path.display().to_string(),
            overwrote: false,
            errors: vec![format!("Could not move the file into place: {error}")],
            exists,
        };
    }

    AuthorSaveResult {
        ok: true,
        id: spec.id.clone(),
        path: path.display().to_string(),
        overwrote: exists,
        errors: Vec::new(),
        exists: false,
    }
}

// === Recommendations ==========================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    #[serde(rename = "type")]
    pub rec_type: String,
    pub id: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationsValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Deserialize)]
struct RecommendationsFile {
    #[serde(default)]
    recommendations: Vec<Recommendation>,
}

/// Validate the YAML block the AI returns in `recommend` mode: a list of
/// catalog items (`{ type, id, reason, confidence }`), optionally nested
/// under a top-level `recommendations:` key. Capability ids are checked
/// against the live registry so the AI cannot invent mods; collections
/// have no local registry yet and are only shape-checked.
#[tauri::command]
pub fn validate_recommendations_yaml(request: AuthorYamlRequest) -> RecommendationsValidationResult {
    let parsed: Result<Vec<Recommendation>, _> = serde_yaml::from_str(&request.yaml);
    let recommendations = parsed.or_else(|_| {
        serde_yaml::from_str::<RecommendationsFile>(&request.yaml)
            .map(|file| file.recommendations)
    });

    let recommendations = match recommendations {
        Ok(recommendations) => recommendations,
        Err(error) => {
            return RecommendationsValidationResult {
                ok: false,
                errors: vec![format!(
                    "Not a recommendations list. Expected `[{{ type, id, reason, confidence }}]`: {error}"
                )],
                recommendations: Vec::new(),
            };
        }
    };

    if recommendations.is_empty() {
        return RecommendationsValidationResult {
            ok: false,
            errors: vec!["The list is empty — ask the AI to pick at least one item.".to_owned()],
            recommendations: Vec::new(),
        };
    }

    let registry = CapabilityRegistry::load();
    let mut errors = Vec::new();
    for (index, rec) in recommendations.iter().enumerate() {
        if rec.rec_type != "capability" && rec.rec_type != "collection" {
            errors.push(format!(
                "item {} has unknown type \"{}\" (use \"capability\" or \"collection\").",
                index + 1,
                rec.rec_type
            ));
            continue;
        }
        if rec.id.is_empty() {
            errors.push(format!("item {} has an empty id.", index + 1));
            continue;
        }
        if rec.rec_type == "capability" && registry.get(&rec.id).is_none() {
            errors.push(format!(
                "item {} references unknown capability \"{}\" — it is not in the Moddin catalog.",
                index + 1,
                rec.id
            ));
        }
    }

    RecommendationsValidationResult {
        ok: errors.is_empty(),
        errors,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MINIMAL_OK: &str = r#"
id: test-mod
displayName: Test Mod
category: qol
status: available
install:
  - kind: write-text-file
    description: Write a marker file.
    params:
      pathField: marker
verify:
  - id: marker-exists
    label: marker exists
    kind: file-exists
    params:
      pathField: marker
configSchema:
  - name: marker
    type: path
    required: true
"#;

    #[test]
    fn validate_accepts_minimal_spec() {
        let result = validate_capability_yaml(AuthorYamlRequest {
            yaml: MINIMAL_OK.to_owned(),
        });
        assert!(result.ok, "errors: {:?}", result.errors);
        let spec = result.spec.expect("summary present");
        assert_eq!(spec.id, "test-mod");
        assert_eq!(spec.install_steps, 1);
        assert_eq!(spec.uninstall_steps, 0);
        assert_eq!(spec.verify_checks, 1);
        assert_eq!(spec.config_fields, vec!["marker".to_owned()]);
    }

    #[test]
    fn validate_reports_schema_and_semantic_errors_together() {
        let yaml = r#"
id: Bad Id!
displayName: Broken
category: fun
status: beta
install:
  - kind: summon-cthulhu
"#;
        let result = validate_capability_yaml(AuthorYamlRequest {
            yaml: yaml.to_owned(),
        });
        assert!(!result.ok);
        let joined = result.errors.join("\n");
        assert!(joined.contains("id \"Bad Id!\""), "{}", joined);
        assert!(joined.contains("category \"fun\""), "{}", joined);
        assert!(joined.contains("status \"beta\""), "{}", joined);
        assert!(joined.contains("unknown kind \"summon-cthulhu\""), "{}", joined);
        assert!(result.spec.is_none());
    }

    #[test]
    fn validate_rejects_garbage_yaml() {
        let result = validate_capability_yaml(AuthorYamlRequest {
            yaml: "not: [valid".to_owned(),
        });
        assert!(!result.ok);
        assert!(result.errors[0].contains("Not valid YAML"));
    }

    #[test]
    fn validate_flags_unknown_config_type_and_empty_enum() {
        let yaml = r#"
id: cfg-check
displayName: Cfg
category: vr
status: planned
configSchema:
  - name: mode
    type: enum
  - name: boost
    type: turbo
checks:
  - id: c
    label: c
    kind: file-exists
    params: { path: "x" }
"#;
        let result = validate_capability_yaml(AuthorYamlRequest {
            yaml: yaml.to_owned(),
        });
        assert!(!result.ok);
        let joined = result.errors.join("\n");
        assert!(joined.contains("unknown type \"turbo\""), "{}", joined);
        assert!(joined.contains("lists no enumValues"), "{}", joined);
    }

    #[test]
    fn preview_plan_summarizes_steps_and_rollback() {
        let result = preview_capability_plan(AuthorYamlRequest {
            yaml: MINIMAL_OK.to_owned(),
        });
        assert!(result.ok);
        assert!(result.plan.contains("Test Mod (test-mod)"), "{}", result.plan);
        assert!(result.plan.contains("1. Write a marker file."), "{}", result.plan);
        assert!(result.plan.contains("rolls back the recorded install"), "{}", result.plan);
        assert!(result.plan.contains("Moddin will ask for: marker"), "{}", result.plan);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "moddin-authoring-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn save_writes_file_and_reports_path() {
        let dir = temp_dir("save-ok");
        let result = save_capability_to_dir(&dir, MINIMAL_OK, false);
        assert!(result.ok, "errors: {:?}", result.errors);
        assert!(!result.overwrote);
        assert!(result.path.ends_with("test-mod.yaml"));
        let written = std::fs::read_to_string(dir.join("test-mod.yaml")).expect("written");
        assert!(written.contains("id: test-mod"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_refuses_builtin_id_even_with_overwrite() {
        let dir = temp_dir("save-builtin");
        let yaml = r#"
id: ofxr-bridge
displayName: Sneaky override
category: vr
status: available
install:
  - kind: file-delete
    params: { pathField: x }
"#;
        let result = save_capability_to_dir(&dir, yaml, true);
        assert!(!result.ok);
        assert!(result.errors[0].contains("built into Moddin"), "{}", result.errors[0]);
        assert!(!dir.join("ofxr-bridge.yaml").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_requires_overwrite_flag_and_backs_up_previous_file() {
        let dir = temp_dir("save-overwrite");
        let first = save_capability_to_dir(&dir, MINIMAL_OK, false);
        assert!(first.ok);

        let mut replaced = String::from(MINIMAL_OK);
        replaced = replaced.replace("Test Mod", "Test Mod v2");

        let refused = save_capability_to_dir(&dir, &replaced, false);
        assert!(!refused.ok);
        assert!(refused.exists, "refusal must flag exists: {:?}", refused.errors);

        let second = save_capability_to_dir(&dir, &replaced, true);
        assert!(second.ok, "errors: {:?}", second.errors);
        assert!(second.overwrote);

        let backup = std::fs::read_to_string(dir.join("test-mod.yaml.bak")).expect("backup");
        assert!(backup.contains("Test Mod\n") || backup.contains("Test Mod"), "{}", backup);
        let current = std::fs::read_to_string(dir.join("test-mod.yaml")).expect("current");
        assert!(current.contains("Test Mod v2"));
        assert!(!dir.join("test-mod.yaml.tmp").exists(), "no tmp left behind");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_rejects_invalid_yaml_without_writing() {
        let dir = temp_dir("save-invalid");
        let result = save_capability_to_dir(&dir, "id: [broken", false);
        assert!(!result.ok);
        assert!(std::fs::read_dir(&dir).map(|mut e| e.next().is_none()).unwrap_or(true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recommendations_validate_against_registry() {
        let yaml = r#"
recommendations:
  - type: capability
    id: optiscaler
    reason: Upscaler swap
    confidence: 0.9
  - type: collection
    id: vr-essentials
    reason: Bundle
"#;
        let result = validate_recommendations_yaml(AuthorYamlRequest {
            yaml: yaml.to_owned(),
        });
        assert!(result.ok, "errors: {:?}", result.errors);
        assert_eq!(result.recommendations.len(), 2);
        assert_eq!(result.recommendations[0].rec_type, "capability");
    }

    #[test]
    fn recommendations_reject_unknown_capability_id() {
        let yaml = r#"
- type: capability
  id: no-such-mod
  reason: hallucinated
  confidence: 0.4
"#;
        let result = validate_recommendations_yaml(AuthorYamlRequest {
            yaml: yaml.to_owned(),
        });
        assert!(!result.ok);
        assert!(result.errors[0].contains("unknown capability \"no-such-mod\""));
    }

    #[test]
    fn recommendations_reject_bad_shape() {
        let result = validate_recommendations_yaml(AuthorYamlRequest {
            yaml: "just a string".to_owned(),
        });
        assert!(!result.ok);
    }
}
