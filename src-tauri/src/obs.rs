use crate::transaction::{self, TransactionRecord};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsVrRequest {
    pub game_id: String,
    pub game_name: String,
    pub collection_name: String,
    pub scene_name: String,
    pub source_name: String,
    pub executable_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsVrPreview {
    pub can_apply: bool,
    pub obs_running: bool,
    pub collection_file: Option<String>,
    pub collection_name: Option<String>,
    pub scene_found: bool,
    pub template_source_name: Option<String>,
    pub source_exists: bool,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

struct CollectionDocument {
    path: PathBuf,
    value: Value,
}

fn obs_scene_root() -> Result<PathBuf, String> {
    let appdata = env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "APPDATA is not available on this Windows user.".to_owned())?;
    Ok(appdata.join("obs-studio").join("basic").join("scenes"))
}

fn is_game_capture(source: &Value) -> bool {
    source
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| id.starts_with("game_capture"))
        || source
            .get("versioned_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("game_capture"))
}

fn has_scene(document: &Value, scene_name: &str) -> bool {
    document
        .get("sources")
        .and_then(Value::as_array)
        .is_some_and(|sources| {
            sources.iter().any(|source| {
                source.get("id").and_then(Value::as_str) == Some("scene")
                    && source.get("name").and_then(Value::as_str) == Some(scene_name)
            })
        })
}

fn locate_collection(request: &ObsVrRequest) -> Result<Option<CollectionDocument>, String> {
    let root = obs_scene_root()?;
    if !root.is_dir() {
        return Ok(None);
    }

    let mut matches = Vec::new();

    for entry in fs::read_dir(&root)
        .map_err(|error| format!("Could not read OBS scene collections: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not read OBS scene entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }

        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&contents) else {
            continue;
        };
        if has_scene(&value, &request.scene_name) {
            matches.push(CollectionDocument { path, value });
        }
    }

    if matches.is_empty() {
        return Ok(None);
    }

    if let Some(index) = matches.iter().position(|document| {
        document.value.get("name").and_then(Value::as_str) == Some(request.collection_name.as_str())
    }) {
        return Ok(Some(matches.swap_remove(index)));
    }

    if matches.len() == 1 {
        return Ok(matches.pop());
    }

    let names = matches
        .iter()
        .filter_map(|document| document.value.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(", ");

    Err(format!(
        "Several OBS collections contain scene '{}', but none is named '{}': {}",
        request.scene_name, request.collection_name, names
    ))
}

fn find_scene_index(sources: &[Value], scene_name: &str) -> Option<usize> {
    sources.iter().position(|source| {
        source.get("id").and_then(Value::as_str) == Some("scene")
            && source.get("name").and_then(Value::as_str) == Some(scene_name)
    })
}

fn find_source_index_by_item(sources: &[Value], item: &Value) -> Option<usize> {
    if let Some(uuid) = item.get("source_uuid").and_then(Value::as_str) {
        if let Some(index) = sources
            .iter()
            .position(|source| source.get("uuid").and_then(Value::as_str) == Some(uuid))
        {
            return Some(index);
        }
    }

    let name = item.get("name").and_then(Value::as_str)?;
    sources
        .iter()
        .position(|source| source.get("name").and_then(Value::as_str) == Some(name))
}

fn find_template(sources: &[Value], scene_index: usize) -> Option<(Value, Value)> {
    let items = sources
        .get(scene_index)?
        .get("settings")?
        .get("items")?
        .as_array()?;

    for item in items {
        let Some(source_index) = find_source_index_by_item(sources, item) else {
            continue;
        };
        let Some(source) = sources.get(source_index) else {
            continue;
        };
        if is_game_capture(source) {
            return Some((item.clone(), source.clone()));
        }
    }

    None
}

fn inspect_document(document: &Value, request: &ObsVrRequest) -> Result<(bool, Option<String>, bool), String> {
    let sources = document
        .get("sources")
        .and_then(Value::as_array)
        .ok_or_else(|| "OBS collection does not contain a sources array.".to_owned())?;

    let Some(scene_index) = find_scene_index(sources, &request.scene_name) else {
        return Ok((false, None, false));
    };

    let template_name = find_template(sources, scene_index)
        .and_then(|(_, source)| source.get("name").and_then(Value::as_str).map(str::to_owned));

    let source_exists = sources.iter().any(|source| {
        source.get("name").and_then(Value::as_str) == Some(request.source_name.as_str())
    });

    Ok((true, template_name, source_exists))
}

fn is_process_running(image_name: &str) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {image_name}"), "/FO", "CSV", "/NH"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let Ok(output) = output else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout)
        .to_ascii_lowercase()
        .contains(&image_name.to_ascii_lowercase())
}

fn running_obs_executable() -> Option<PathBuf> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-Process obs64 -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Path)",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn close_obs_gracefully() -> Result<bool, String> {
    if !is_process_running("obs64.exe") {
        return Ok(false);
    }

    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "$p = Get-Process obs64 -ErrorAction SilentlyContinue; if ($p) { $p | ForEach-Object { [void]$_.CloseMainWindow() } }",
        ])
        .status()
        .map_err(|error| format!("Could not ask OBS to close: {error}"))?;

    if !status.success() {
        return Err("Windows could not request a graceful OBS shutdown.".to_owned());
    }

    for _ in 0..40 {
        if !is_process_running("obs64.exe") {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(500));
    }

    Err("OBS did not close within 20 seconds. Close OBS manually and try again.".to_owned())
}

fn reopen_obs(path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    if !path.is_file() {
        return;
    }

    let mut command = Command::new(path);
    if let Some(parent) = path.parent() {
        command.current_dir(parent);
    }
    let _ = command.spawn();
}

fn encode_obs_window_part(value: &str) -> String {
    value.replace('#', "#22").replace(':', "#3A")
}

fn ensure_object(value: &mut Value) -> Result<&mut Map<String, Value>, String> {
    if !value.is_object() {
        *value = json!({});
    }
    value
        .as_object_mut()
        .ok_or_else(|| "Expected a JSON object.".to_owned())
}

fn configure_document(document: &mut Value, request: &ObsVrRequest) -> Result<(), String> {
    let sources_read = document
        .get("sources")
        .and_then(Value::as_array)
        .ok_or_else(|| "OBS collection does not contain a sources array.".to_owned())?;

    let scene_index = find_scene_index(sources_read, &request.scene_name)
        .ok_or_else(|| format!("OBS scene '{}' was not found.", request.scene_name))?;

    let (template_item, template_source) = find_template(sources_read, scene_index)
        .ok_or_else(|| format!("Scene '{}' has no Game Capture source to use as a template.", request.scene_name))?;

    let existing_source_index = sources_read.iter().position(|source| {
        source.get("name").and_then(Value::as_str) == Some(request.source_name.as_str())
    });

    let sources = document
        .get_mut("sources")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "OBS collection sources became invalid.".to_owned())?;

    let target_index = if let Some(index) = existing_source_index {
        index
    } else {
        let mut source = template_source;
        let source_object = ensure_object(&mut source)?;
        source_object.insert("name".to_owned(), Value::String(request.source_name.clone()));
        source_object.insert("uuid".to_owned(), Value::String(uuid::Uuid::new_v4().to_string()));
        source_object.insert("hotkeys".to_owned(), json!({}));
        sources.push(source);
        sources.len() - 1
    };

    let target_uuid = {
        let source = sources
            .get_mut(target_index)
            .ok_or_else(|| "Target OBS source index is invalid.".to_owned())?;
        let source_object = ensure_object(source)?;
        source_object.insert("name".to_owned(), Value::String(request.source_name.clone()));

        let uuid = source_object
            .get("uuid")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        source_object.insert("uuid".to_owned(), Value::String(uuid.clone()));

        let settings = source_object.entry("settings".to_owned()).or_insert_with(|| json!({}));
        let settings_object = ensure_object(settings)?;
        settings_object.insert("capture_mode".to_owned(), Value::String("window".to_owned()));
        settings_object.insert(
            "window".to_owned(),
            Value::String(format!("::{}", encode_obs_window_part(&request.executable_name))),
        );
        settings_object.insert("priority".to_owned(), Value::Number(2.into()));
        uuid
    };

    let scene = sources
        .get_mut(scene_index)
        .ok_or_else(|| "OBS scene index became invalid.".to_owned())?;
    let scene_object = ensure_object(scene)?;
    let settings = scene_object.entry("settings".to_owned()).or_insert_with(|| json!({}));
    let settings_object = ensure_object(settings)?;
    let items_value = settings_object.entry("items".to_owned()).or_insert_with(|| json!([]));
    let items = items_value
        .as_array_mut()
        .ok_or_else(|| "OBS scene items are not an array.".to_owned())?;

    if let Some(item) = items.iter_mut().find(|item| {
        item.get("source_uuid").and_then(Value::as_str) == Some(target_uuid.as_str())
            || item.get("name").and_then(Value::as_str) == Some(request.source_name.as_str())
    }) {
        let item_object = ensure_object(item)?;
        item_object.insert("name".to_owned(), Value::String(request.source_name.clone()));
        item_object.insert("source_uuid".to_owned(), Value::String(target_uuid));
        item_object.insert("visible".to_owned(), Value::Bool(true));
    } else {
        let mut item = template_item;
        let max_id = items
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_i64))
            .max()
            .unwrap_or(0);

        let item_object = ensure_object(&mut item)?;
        item_object.insert("id".to_owned(), Value::Number((max_id + 1).into()));
        item_object.insert("name".to_owned(), Value::String(request.source_name.clone()));
        item_object.insert("source_uuid".to_owned(), Value::String(target_uuid));
        item_object.insert("visible".to_owned(), Value::Bool(true));
        items.push(item);
    }

    Ok(())
}

#[tauri::command]
pub fn preview_obs_vr(request: ObsVrRequest) -> Result<ObsVrPreview, String> {
    let obs_running = is_process_running("obs64.exe");
    let Some(document) = locate_collection(&request)? else {
        return Ok(ObsVrPreview {
            can_apply: false,
            obs_running,
            collection_file: None,
            collection_name: None,
            scene_found: false,
            template_source_name: None,
            source_exists: false,
            changes: Vec::new(),
            warnings: vec![format!(
                "No OBS collection containing scene '{}' was found.",
                request.scene_name
            )],
        });
    };

    let (scene_found, template_source_name, source_exists) = inspect_document(&document.value, &request)?;
    let collection_name = document
        .value
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned);

    let mut changes = vec![format!(
        "Back up OBS collection file '{}' before changing it.",
        document.path.display()
    )];

    if source_exists {
        changes.push(format!(
            "Update '{}' to target {} by executable.",
            request.source_name, request.executable_name
        ));
    } else if let Some(template) = &template_source_name {
        changes.push(format!(
            "Clone Game Capture '{}' into scene '{}' as '{}'.",
            template, request.scene_name, request.source_name
        ));
        changes.push(format!("Target executable: {}.", request.executable_name));
    }

    let mut warnings = Vec::new();
    if obs_running {
        warnings.push("OBS is running. Moddin will close it gracefully before editing and reopen it afterwards.".to_owned());
    }
    if template_source_name.is_none() && !source_exists {
        warnings.push(format!(
            "Scene '{}' has no existing Game Capture source to clone.",
            request.scene_name
        ));
    }

    Ok(ObsVrPreview {
        can_apply: scene_found && (source_exists || template_source_name.is_some()),
        obs_running,
        collection_file: Some(document.path.to_string_lossy().into_owned()),
        collection_name,
        scene_found,
        template_source_name,
        source_exists,
        changes,
        warnings,
    })
}

#[tauri::command]
pub fn configure_obs_vr(request: ObsVrRequest) -> Result<TransactionRecord, String> {
    let mut document = locate_collection(&request)?
        .ok_or_else(|| format!("No OBS collection containing scene '{}' was found.", request.scene_name))?;

    let obs_path = running_obs_executable().or_else(|| {
        let default = PathBuf::from(r"C:\Program Files\obs-studio\bin\64bit\obs64.exe");
        default.is_file().then_some(default)
    });
    let obs_was_open = close_obs_gracefully()?;

    let transaction = transaction::backup_file(
        &document.path,
        "obs-vr",
        &format!("Configure OBS VR for {}", request.game_name),
        &request.game_id,
    )?;

    let result = (|| -> Result<(), String> {
        configure_document(&mut document.value, &request)?;
        let json = serde_json::to_string_pretty(&document.value)
            .map_err(|error| format!("Could not serialize OBS collection: {error}"))?;
        fs::write(&document.path, json)
            .map_err(|error| format!("Could not write OBS collection '{}': {error}", document.path.display()))?;

        let verify = fs::read_to_string(&document.path)
            .map_err(|error| format!("Could not verify OBS collection: {error}"))?;
        let verify: Value = serde_json::from_str(&verify)
            .map_err(|error| format!("OBS collection failed JSON validation after write: {error}"))?;
        let (_, _, source_exists) = inspect_document(&verify, &request)?;
        if !source_exists {
            return Err(format!("OBS source '{}' was not present after saving.", request.source_name));
        }

        Ok(())
    })();

    if let Err(error) = result {
        let restore_result = transaction::restore_record(transaction.clone());
        if obs_was_open {
            reopen_obs(obs_path.as_deref());
        }
        return match restore_result {
            Ok(_) => Err(format!("{error} The OBS backup was restored automatically.")),
            Err(restore_error) => Err(format!(
                "{error} Automatic restore also failed: {restore_error}"
            )),
        };
    }

    if obs_was_open {
        reopen_obs(obs_path.as_deref());
    }

    Ok(transaction)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_collection() -> Value {
        json!({
            "name": "Sem nome",
            "sources": [
                {
                    "id": "game_capture",
                    "name": "Captura de jogo",
                    "uuid": "template-uuid",
                    "settings": { "capture_mode": "window", "window": "Old:Class:old.exe" },
                    "hotkeys": {}
                },
                {
                    "id": "scene",
                    "name": "vr",
                    "uuid": "scene-uuid",
                    "settings": {
                        "items": [
                            {
                                "id": 1,
                                "name": "Captura de jogo",
                                "source_uuid": "template-uuid",
                                "visible": true,
                                "pos": { "x": 12.0, "y": 8.0 }
                            }
                        ]
                    }
                }
            ]
        })
    }

    #[test]
    fn clones_game_capture_and_preserves_scene_transform() {
        let mut collection = sample_collection();
        let request = ObsVrRequest {
            game_id: "elden-ring".to_owned(),
            game_name: "Elden Ring".to_owned(),
            collection_name: "Sem nome".to_owned(),
            scene_name: "vr".to_owned(),
            source_name: "Elden Ring VR".to_owned(),
            executable_name: "eldenring.exe".to_owned(),
        };

        configure_document(&mut collection, &request).expect("configure");
        let sources = collection["sources"].as_array().expect("sources");
        let target = sources
            .iter()
            .find(|source| source["name"] == "Elden Ring VR")
            .expect("target source");
        assert_eq!(target["settings"]["window"], "::eldenring.exe");
        assert_eq!(target["settings"]["priority"], 2);

        let scene = sources
            .iter()
            .find(|source| source["name"] == "vr")
            .expect("scene");
        let item = scene["settings"]["items"]
            .as_array()
            .expect("items")
            .iter()
            .find(|item| item["name"] == "Elden Ring VR")
            .expect("scene item");
        assert_eq!(item["pos"]["x"], 12.0);
        assert_eq!(item["pos"]["y"], 8.0);
    }

    #[test]
    fn skips_non_resolvable_scene_items_when_searching_for_template() {
        let mut collection = sample_collection();
        let scene = collection["sources"]
            .as_array_mut()
            .expect("sources")
            .iter_mut()
            .find(|source| source["name"] == "vr")
            .expect("scene");
        scene["settings"]["items"]
            .as_array_mut()
            .expect("items")
            .insert(0, json!({ "id": 99, "name": "Missing source", "source_uuid": "missing-uuid" }));

        let sources = collection["sources"].as_array().expect("sources");
        let scene_index = find_scene_index(sources, "vr").expect("scene index");
        let (_, source) = find_template(sources, scene_index).expect("game capture template");
        assert_eq!(source["name"], "Captura de jogo");
    }
}
