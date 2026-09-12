use crate::{
    inspection,
    transaction::{self, TransactionRecord},
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    env,
    fs::{self, File},
    io::Cursor,
    path::{Component, Path, PathBuf},
    time::Duration,
};
use zip::ZipArchive;

const MARKER_FILE: &str = ".moddin-uevr.json";
const UEVR_NIGHTLY_API: &str = "https://api.github.com/repos/praydog/UEVR-nightly/releases/latest";
const JOEY_API: &str = "https://api.github.com/repos/joeyhodge/UEVR/releases/latest";
const AFW_API: &str = "https://api.github.com/repos/PureDark/UEVR/releases/latest";
const UEVR_DOWNLOAD_PREFIX: &str = "https://github.com/praydog/UEVR-nightly/releases/download/";
const JOEY_DOWNLOAD_PREFIX: &str = "https://github.com/joeyhodge/UEVR/releases/download/";
const AFW_DOWNLOAD_PREFIX: &str = "https://github.com/PureDark/UEVR/releases/download/";
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UevrRequest {
    pub game_id: String,
    pub game_name: String,
    pub install_dir: String,
    pub executable: String,
    pub backend: String,
    pub version_policy: String,
    pub release_api_url: String,
    pub release_tag: Option<String>,
    pub backend_release_api_url: Option<String>,
    pub backend_release_tag: Option<String>,
    #[serde(default)]
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UevrPreview {
    pub can_apply: bool,
    pub game_running: bool,
    pub uevr_running: bool,
    pub executable_exists: bool,
    pub executable_path: String,
    pub executable_directory: String,
    pub engine: Option<String>,
    pub engine_version: Option<String>,
    pub engine_confidence: String,
    pub engine_evidence: Vec<String>,
    pub backend: String,
    pub backend_label: String,
    pub selected_version: Option<String>,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub installed_backend: Option<String>,
    pub install_directory: String,
    pub manual_install_detected: bool,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UevrMarker {
    version: String,
    backend: String,
    base_sha256: Option<String>,
    backend_sha256: Option<String>,
    installed_files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Clone)]
struct ResolvedPackage {
    version: String,
    release_url: Option<String>,
    archive_url: String,
    archive_name: String,
    checksum_url: Option<String>,
    backend_url: Option<String>,
    backend_name: Option<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct ArchiveFile {
    archive_name: String,
    relative_path: String,
}

fn tools_root() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("Moddin")
        .join("tools")
        .join("uevr")
}

fn backend_directory(backend: &str) -> PathBuf {
    tools_root().join(match backend {
        "nightly" => "nightly",
        "joey" => "joey",
        "afw" => "afw",
        "joey-afw" => "joey-afw",
        _ => "invalid-backend",
    })
}

fn marker_path(backend: &str) -> PathBuf {
    backend_directory(backend).join(MARKER_FILE)
}

fn read_marker(backend: &str) -> Option<UevrMarker> {
    let contents = fs::read_to_string(marker_path(backend)).ok()?;
    serde_json::from_str(&contents).ok()
}

fn safe_join_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    if relative_path.as_os_str().is_empty() {
        return Err("UEVR path is empty.".to_owned());
    }

    for component in relative_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "UEVR archive path escapes its install directory: {relative}"
                ));
            }
        }
    }

    Ok(root.join(relative_path))
}

fn valid_release_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 160
        && tag
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

fn validate_api_url(url: &str, expected: &str) -> Result<(), String> {
    if url == expected {
        Ok(())
    } else {
        Err(format!(
            "UEVR release source must be the official GitHub endpoint: {expected}"
        ))
    }
}

fn validate_request(request: &UevrRequest) -> Result<(), String> {
    if request.game_id.trim().is_empty() || request.game_name.trim().is_empty() {
        return Err("UEVR recipe must identify the game.".to_owned());
    }
    if !matches!(
        request.backend.as_str(),
        "nightly" | "joey" | "afw" | "joey-afw"
    ) {
        return Err(format!("Unsupported UEVR backend: {}", request.backend));
    }
    if !matches!(request.version_policy.as_str(), "latest" | "pinned") {
        return Err("UEVR version policy must be latest or pinned.".to_owned());
    }
    if request.version_policy == "pinned"
        && !request
            .release_tag
            .as_deref()
            .is_some_and(valid_release_tag)
    {
        return Err("A pinned UEVR recipe must provide a safe release tag.".to_owned());
    }
    let base_api = if request.backend == "afw" || request.backend == "joey-afw" {
        AFW_API
    } else {
        UEVR_NIGHTLY_API
    };
    validate_api_url(&request.release_api_url, base_api)?;

    if request.backend == "joey" {
        let api = request
            .backend_release_api_url
            .as_deref()
            .ok_or_else(|| "The Joey backend requires its official release endpoint.".to_owned())?;
        validate_api_url(api, JOEY_API)?;
        if let Some(tag) = request.backend_release_tag.as_deref() {
            if !valid_release_tag(tag) {
                return Err("The pinned Joey backend tag is invalid.".to_owned());
            }
        }
    }

    Ok(())
}

fn executable_context(request: &UevrRequest) -> Result<(PathBuf, PathBuf, String), String> {
    let install_root = PathBuf::from(&request.install_dir);
    if !install_root.is_dir() {
        return Err(format!(
            "Game install directory does not exist: {}",
            request.install_dir
        ));
    }

    let executable_path = safe_join_relative(&install_root, &request.executable)?;
    let executable_directory = executable_path
        .parent()
        .ok_or_else(|| "Could not resolve the game executable directory.".to_owned())?
        .to_path_buf();
    let process_name = executable_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Could not resolve the game executable name.".to_owned())?
        .to_owned();
    Ok((executable_path, executable_directory, process_name))
}

fn is_process_running(image_name: &str) -> bool {
    crate::process::is_process_running(image_name)
}

fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent("Moddin/0.1 (+https://github.com/marcoasjunior/moddin)")
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| format!("Could not create secure UEVR download client: {error}"))
}

async fn fetch_release(
    client: &Client,
    api_url: &str,
    tag: Option<&str>,
) -> Result<GithubRelease, String> {
    let endpoint = if let Some(tag) = tag {
        let base = api_url
            .strip_suffix("/latest")
            .ok_or_else(|| "UEVR release endpoint cannot be pinned.".to_owned())?;
        format!("{base}/tags/{tag}")
    } else {
        api_url.to_owned()
    };

    client
        .get(endpoint)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|error| format!("Could not resolve the UEVR release: {error}"))?
        .error_for_status()
        .map_err(|error| format!("UEVR release source returned an error: {error}"))?
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("Could not parse UEVR release metadata: {error}"))
}

fn find_asset<'a>(release: &'a GithubRelease, name: &str) -> Option<&'a GithubAsset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(name))
}

fn find_asset_prefix<'a>(release: &'a GithubRelease, prefix: &str) -> Option<&'a GithubAsset> {
    release.assets.iter().find(|asset| {
        asset
            .name
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase())
            && asset.name.to_ascii_lowercase().ends_with(".zip")
    })
}

fn validate_asset(asset: &GithubAsset, prefix: &str) -> Result<(), String> {
    if asset.size == 0 || asset.size > MAX_DOWNLOAD_BYTES {
        return Err(format!(
            "UEVR asset '{}' has an unsafe advertised size.",
            asset.name
        ));
    }
    if !asset.browser_download_url.starts_with(prefix) {
        return Err(format!(
            "UEVR asset '{}' is not from the official release host.",
            asset.name
        ));
    }
    Ok(())
}

async fn resolve_package(request: &UevrRequest, http: &Client) -> Result<ResolvedPackage, String> {
    let tag = request.release_tag.as_deref();
    let release = fetch_release(http, &request.release_api_url, tag).await?;
    let mut warnings = Vec::new();

    if request.backend == "afw" || request.backend == "joey-afw" {
        let prefix = if request.backend == "joey-afw" {
            "UEVR-joeyhodge_AFW"
        } else {
            "UEVR-nightly_AFW"
        };
        let asset = find_asset_prefix(&release, prefix)
            .ok_or_else(|| format!("The AFW release has no {prefix} archive."))?;
        validate_asset(asset, AFW_DOWNLOAD_PREFIX)?;
        let checksum_url = find_asset(&release, &format!("{}.sha256", asset.name))
            .map(|checksum| checksum.browser_download_url.clone());
        if checksum_url.is_none() {
            warnings.push(
                "This AFW release does not publish a SHA-256 sidecar; Moddin will record the downloaded hash and the official GitHub source.".to_owned(),
            );
        }
        return Ok(ResolvedPackage {
            version: release.tag_name.clone(),
            release_url: release.html_url.clone(),
            archive_url: asset.browser_download_url.clone(),
            archive_name: asset.name.clone(),
            checksum_url,
            backend_url: None,
            backend_name: None,
            warnings,
        });
    }

    let archive = find_asset(&release, "uevr.zip")
        .ok_or_else(|| "The UEVR nightly release has no uevr.zip asset.".to_owned())?;
    validate_asset(archive, UEVR_DOWNLOAD_PREFIX)?;
    let checksum = find_asset(&release, "uevr.zip.sha256")
        .ok_or_else(|| "The UEVR nightly release has no uevr.zip.sha256 asset.".to_owned())?;
    validate_asset(checksum, UEVR_DOWNLOAD_PREFIX)?;

    let (backend_url, backend_name, version) = if request.backend == "joey" {
        let api = request
            .backend_release_api_url
            .as_deref()
            .ok_or_else(|| "The Joey backend release endpoint is missing.".to_owned())?;
        let backend_release =
            fetch_release(http, api, request.backend_release_tag.as_deref()).await?;
        let backend = find_asset(&backend_release, "UEVRBackend.dll")
            .ok_or_else(|| "The Joey release has no UEVRBackend.dll asset.".to_owned())?;
        validate_asset(backend, JOEY_DOWNLOAD_PREFIX)?;
        warnings.push(
            "The Joey backend release does not publish a checksum sidecar; Moddin records its downloaded hash and official GitHub source.".to_owned(),
        );
        (
            Some(backend.browser_download_url.clone()),
            Some(backend.name.clone()),
            format!("{} + {}", release.tag_name, backend_release.tag_name),
        )
    } else {
        (None, None, release.tag_name.clone())
    };

    Ok(ResolvedPackage {
        version,
        release_url: release.html_url.clone(),
        archive_url: archive.browser_download_url.clone(),
        archive_name: archive.name.clone(),
        checksum_url: Some(checksum.browser_download_url.clone()),
        backend_url,
        backend_name,
        warnings,
    })
}

async fn download(http: &Client, url: &str) -> Result<Vec<u8>, String> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not download UEVR from GitHub: {error}"))?
        .error_for_status()
        .map_err(|error| format!("GitHub returned an error while downloading UEVR: {error}"))?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_DOWNLOAD_BYTES)
    {
        return Err("UEVR download exceeds the safe size limit.".to_owned());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read the UEVR download: {error}"))?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err("UEVR download exceeds the safe size limit.".to_owned());
    }
    Ok(bytes.to_vec())
}

fn checksum_from_sidecar(bytes: &[u8]) -> Result<String, String> {
    let text = String::from_utf8_lossy(bytes);
    let checksum = text
        .split_whitespace()
        .next()
        .ok_or_else(|| "The UEVR checksum sidecar is empty.".to_owned())?;
    if checksum.len() != 64
        || !checksum
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("The UEVR checksum sidecar is invalid.".to_owned());
    }
    Ok(checksum.to_ascii_lowercase())
}

fn normalized_archive_path(raw: &str) -> Result<Vec<String>, String> {
    let raw = raw.replace('\\', "/");
    if raw.starts_with('/') || raw.contains(':') {
        return Err(format!("UEVR archive contains an absolute path: {raw}"));
    }

    let mut components = Vec::new();
    for component in raw.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            return Err(format!("UEVR archive contains a parent path: {raw}"));
        }
        components.push(component.to_owned());
    }
    if components.is_empty() {
        return Err("UEVR archive contains an empty file path.".to_owned());
    }
    Ok(components)
}

fn archive_file_plan(bytes: &[u8]) -> Result<Vec<ArchiveFile>, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Could not inspect the UEVR ZIP: {error}"))?;
    let mut raw_files = Vec::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| format!("Could not inspect a UEVR ZIP entry: {error}"))?;
        if file.is_dir() {
            continue;
        }
        let path = normalized_archive_path(file.name())?;
        raw_files.push((file.name().to_owned(), path));
    }

    let injector_root = raw_files
        .iter()
        .find(|(_, path)| {
            path.last()
                .is_some_and(|name| name.eq_ignore_ascii_case("UEVRInjector.exe"))
        })
        .map(|(_, path)| path[..path.len() - 1].to_vec())
        .ok_or_else(|| "The UEVR archive does not contain UEVRInjector.exe.".to_owned())?;
    let injector = injector_root.len();

    let mut plan = Vec::new();
    let mut seen = HashSet::new();
    for (archive_name, path) in raw_files {
        if path.len() <= injector {
            continue;
        }
        if path[..injector] != injector_root[..] {
            return Err(format!(
                "UEVR archive contains files outside its common install root: {archive_name}"
            ));
        }
        let relative_path = path[injector..].join("/");
        if seen.insert(relative_path.clone()) {
            plan.push(ArchiveFile {
                archive_name,
                relative_path,
            });
        }
    }
    if plan.is_empty() {
        return Err("The UEVR archive contains no installable files.".to_owned());
    }
    Ok(plan)
}

fn extract_archive(bytes: &[u8], target_root: &Path, plan: &[ArchiveFile]) -> Result<(), String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Could not open the UEVR ZIP: {error}"))?;
    for item in plan {
        let mut source = archive.by_name(&item.archive_name).map_err(|error| {
            format!(
                "Could not read UEVR ZIP entry '{}': {error}",
                item.archive_name
            )
        })?;
        let target = safe_join_relative(target_root, &item.relative_path)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create UEVR directory: {error}"))?;
        }
        let mut output = File::create(&target).map_err(|error| {
            format!("Could not create UEVR file '{}': {error}", target.display())
        })?;
        std::io::copy(&mut source, &mut output).map_err(|error| {
            format!(
                "Could not extract UEVR file '{}': {error}",
                target.display()
            )
        })?;
    }
    Ok(())
}

fn directory_has_files(path: &Path) -> bool {
    fs::read_dir(path)
        .ok()
        .is_some_and(|entries| entries.flatten().any(|entry| entry.path().is_file()))
}

fn marker_is_installed(marker: Option<&UevrMarker>, directory: &Path) -> bool {
    marker.is_some_and(|value| {
        value.installed_files.iter().all(|relative| {
            safe_join_relative(directory, relative).is_ok_and(|path| path.is_file())
        })
    })
}

fn local_preview(
    request: &UevrRequest,
    selected_version: Option<&str>,
    package_warnings: &[String],
) -> Result<UevrPreview, String> {
    let (executable_path, executable_directory, process_name) = executable_context(request)?;
    let environment = inspection::inspect_game_environment_sync(
        request.install_dir.clone(),
        request.executable.clone(),
    )?;
    let directory = backend_directory(&request.backend);
    let marker = read_marker(&request.backend);
    let game_running = is_process_running(&process_name);
    let uevr_running = is_process_running("UEVRInjector.exe");
    let manual_install_detected = directory_has_files(&directory) && marker.is_none();
    let installed = marker_is_installed(marker.as_ref(), &directory);
    let mut warnings = request.safety_notes.clone();
    warnings.extend(package_warnings.iter().cloned());

    if environment.engine.as_deref() != Some("Unreal Engine") {
        warnings.push(
            "UEVR only supports Unreal Engine games. The detected engine is not eligible, so nothing will be installed.".to_owned(),
        );
    } else if environment.engine_version.is_none() {
        warnings.push(
            "Unreal Engine was detected, but the minor engine version could not be identified locally; confirm it is within UEVR's supported range before injecting.".to_owned(),
        );
    }
    if game_running {
        warnings.push(format!(
            "{process_name} is running. Close the game before installing UEVR."
        ));
    }
    if uevr_running {
        warnings.push(
            "UEVRInjector.exe is running. Close it before replacing a UEVR build.".to_owned(),
        );
    }
    if manual_install_detected {
        warnings.push(
            "The selected UEVR folder contains files without a Moddin marker. Automatic overwrite is blocked to protect that installation.".to_owned(),
        );
    }
    if installed
        && marker
            .as_ref()
            .is_some_and(|value| value.backend != request.backend)
    {
        warnings.push("The installed UEVR marker belongs to another backend variant.".to_owned());
    }

    let mut changes = vec![
        "Resolve the requested UEVR release from the official GitHub API at install time."
            .to_owned(),
        format!(
            "Install the {} variant in its own Moddin tools directory.",
            backend_label(&request.backend)
        ),
        "Verify archive paths, write the files, and record a rollback transaction.".to_owned(),
    ];
    if let Some(version) = selected_version {
        changes.insert(1, format!("Selected release: {version}."));
    }
    if installed {
        changes.push("Update the existing Moddin-managed UEVR variant.".to_owned());
    }

    Ok(UevrPreview {
        can_apply: environment.executable_exists
            && environment.engine.as_deref() == Some("Unreal Engine")
            && !game_running
            && !uevr_running
            && !manual_install_detected
            && selected_version.is_some(),
        game_running,
        uevr_running,
        executable_exists: environment.executable_exists,
        executable_path: executable_path.to_string_lossy().into_owned(),
        executable_directory: executable_directory.to_string_lossy().into_owned(),
        engine: environment.engine,
        engine_version: environment.engine_version,
        engine_confidence: environment.engine_confidence,
        engine_evidence: environment.engine_evidence,
        backend: request.backend.clone(),
        backend_label: backend_label(&request.backend).to_owned(),
        selected_version: selected_version.map(str::to_owned),
        installed,
        installed_version: marker.as_ref().map(|value| value.version.clone()),
        installed_backend: marker.map(|value| value.backend.clone()),
        install_directory: directory.to_string_lossy().into_owned(),
        manual_install_detected,
        changes,
        warnings,
    })
}

fn backend_label(backend: &str) -> &'static str {
    match backend {
        "nightly" => "UEVR Nightly",
        "joey" => "UEVR Nightly + JoeyHodge backend",
        "afw" => "UEVR AFW",
        "joey-afw" => "UEVR AFW + JoeyHodge backend",
        _ => "Unknown UEVR backend",
    }
}

fn local_preflight_allows_release(preview: &UevrPreview) -> bool {
    preview.executable_exists
        && preview.engine.as_deref() == Some("Unreal Engine")
        && !preview.game_running
        && !preview.uevr_running
        && !preview.manual_install_detected
}

#[tauri::command]
pub async fn preview_uevr(request: UevrRequest) -> Result<UevrPreview, String> {
    validate_request(&request)?;
    let preflight_request = request.clone();
    let preflight =
        tauri::async_runtime::spawn_blocking(move || local_preview(&preflight_request, None, &[]))
            .await
            .map_err(|error| format!("UEVR preflight task failed: {error}"))??;
    if !local_preflight_allows_release(&preflight) {
        return Ok(preflight);
    }

    let http = client()?;
    match resolve_package(&request, &http).await {
        Ok(package) => {
            let preview_request = request.clone();
            tauri::async_runtime::spawn_blocking(move || {
                local_preview(&preview_request, Some(&package.version), &package.warnings)
            })
            .await
            .map_err(|error| format!("UEVR preview task failed: {error}"))?
        }
        Err(error) => {
            let preview_request = request.clone();
            tauri::async_runtime::spawn_blocking(move || {
                local_preview(
                    &preview_request,
                    None,
                    &[format!(
                        "Could not resolve the selected UEVR release: {error}"
                    )],
                )
            })
            .await
            .map_err(|error| format!("UEVR preview task failed: {error}"))?
        }
    }
}

fn install_from_bytes(
    request: UevrRequest,
    package: ResolvedPackage,
    archive_bytes: Vec<u8>,
    backend_bytes: Option<Vec<u8>>,
    expected_archive_sha256: Option<String>,
) -> Result<TransactionRecord, String> {
    let environment = inspection::inspect_game_environment_sync(
        request.install_dir.clone(),
        request.executable.clone(),
    )?;
    let (_, _, process_name) = executable_context(&request)?;
    if !environment.executable_exists || environment.engine.as_deref() != Some("Unreal Engine") {
        return Err("UEVR installation is blocked because the selected game is not detected as Unreal Engine.".to_owned());
    }
    if is_process_running(&process_name) || is_process_running("UEVRInjector.exe") {
        return Err("Close the game and UEVRInjector.exe before installing UEVR.".to_owned());
    }

    let actual_archive_sha256 = format!("{:x}", Sha256::digest(&archive_bytes));
    if let Some(expected) = expected_archive_sha256 {
        if !actual_archive_sha256.eq_ignore_ascii_case(&expected) {
            return Err(format!(
                "UEVR archive SHA-256 mismatch. Expected {expected}, received {actual_archive_sha256}. Nothing was installed."
            ));
        }
    }

    let target_root = backend_directory(&request.backend);
    let marker_target = target_root.join(MARKER_FILE);
    let old_marker = read_marker(&request.backend);
    if directory_has_files(&target_root) && old_marker.is_none() {
        return Err(
            "The selected UEVR directory is not managed by Moddin; overwrite was blocked."
                .to_owned(),
        );
    }
    let plan = archive_file_plan(&archive_bytes)?;
    let mut targets: Vec<PathBuf> = plan
        .iter()
        .map(|file| safe_join_relative(&target_root, &file.relative_path))
        .collect::<Result<_, _>>()?;
    if request.backend == "joey" {
        targets.push(target_root.join("UEVRBackend.dll"));
    }
    targets.push(marker_target.clone());
    if let Some(marker) = &old_marker {
        for relative in &marker.installed_files {
            if let Ok(path) = safe_join_relative(&target_root, relative) {
                targets.push(path);
            }
        }
    }

    let backend_sha256 = backend_bytes
        .as_ref()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)));
    let mut metadata = BTreeMap::new();
    metadata.insert("processName".to_owned(), process_name);
    metadata.insert("version".to_owned(), package.version.clone());
    metadata.insert("backend".to_owned(), request.backend.clone());
    metadata.insert("archiveName".to_owned(), package.archive_name.clone());
    metadata.insert("archiveSha256".to_owned(), actual_archive_sha256.clone());
    metadata.insert("archiveUrl".to_owned(), package.archive_url.clone());
    if let Some(url) = package.release_url.clone() {
        metadata.insert("releaseUrl".to_owned(), url);
    }
    if let Some(url) = package.backend_url.clone() {
        metadata.insert("backendUrl".to_owned(), url);
    }
    if let Some(name) = package.backend_name.clone() {
        metadata.insert("backendName".to_owned(), name);
    }
    if let Some(hash) = &backend_sha256 {
        metadata.insert("backendSha256".to_owned(), hash.clone());
    }

    let transaction = transaction::begin_file_set_transaction(
        &target_root,
        &targets,
        "uevr",
        &format!(
            "Install {} {} for {}",
            backend_label(&request.backend),
            package.version,
            request.game_name
        ),
        &request.game_id,
        metadata,
    )?;

    let apply_result = (|| -> Result<(), String> {
        extract_archive(&archive_bytes, &target_root, &plan)?;
        if let Some(bytes) = backend_bytes {
            let target = target_root.join("UEVRBackend.dll");
            fs::write(&target, bytes)
                .map_err(|error| format!("Could not install Joey UEVRBackend.dll: {error}"))?;
        }

        let mut installed_files: Vec<String> =
            plan.iter().map(|file| file.relative_path.clone()).collect();
        if request.backend == "joey" {
            installed_files.push("UEVRBackend.dll".to_owned());
        }
        installed_files.push(MARKER_FILE.to_owned());
        installed_files.sort();
        installed_files.dedup();
        let marker = UevrMarker {
            version: package.version.clone(),
            backend: request.backend.clone(),
            base_sha256: Some(actual_archive_sha256.clone()),
            backend_sha256,
            installed_files,
        };
        let marker_json = serde_json::to_string_pretty(&marker)
            .map_err(|error| format!("Could not serialize UEVR marker: {error}"))?;
        fs::write(&marker_target, marker_json)
            .map_err(|error| format!("Could not write Moddin UEVR marker: {error}"))?;

        if !marker_is_installed(Some(&marker), &target_root) {
            return Err("UEVR files failed post-install validation.".to_owned());
        }
        Ok(())
    })();

    if let Err(error) = apply_result {
        return match transaction::restore_record(transaction) {
            Ok(_) => Err(format!(
                "{error} All changed files were restored automatically."
            )),
            Err(restore_error) => Err(format!(
                "{error} Automatic rollback also failed: {restore_error}"
            )),
        };
    }

    transaction::mark_applied(transaction)
}

#[tauri::command]
pub async fn install_uevr(request: UevrRequest) -> Result<TransactionRecord, String> {
    validate_request(&request)?;
    let preflight_request = request.clone();
    let preflight =
        tauri::async_runtime::spawn_blocking(move || local_preview(&preflight_request, None, &[]))
            .await
            .map_err(|error| format!("UEVR preflight task failed: {error}"))??;
    if !local_preflight_allows_release(&preflight) {
        return Err(
            "UEVR installation is blocked by the local engine, process, or installation checks."
                .to_owned(),
        );
    }

    let http = client()?;
    let package = resolve_package(&request, &http).await?;
    let archive_bytes = download(&http, &package.archive_url).await?;
    let expected_archive_sha256 = if let Some(url) = package.checksum_url.as_deref() {
        Some(checksum_from_sidecar(&download(&http, url).await?)?)
    } else {
        None
    };
    let backend_bytes = if let Some(url) = package.backend_url.as_deref() {
        Some(download(&http, url).await?)
    } else {
        None
    };

    let request_for_install = request.clone();
    tauri::async_runtime::spawn_blocking(move || {
        install_from_bytes(
            request_for_install,
            package,
            archive_bytes,
            backend_bytes,
            expected_archive_sha256,
        )
    })
    .await
    .map_err(|error| format!("UEVR installer task failed: {error}"))?
}

#[tauri::command]
pub fn uninstall_uevr(request: UevrRequest) -> Result<TransactionRecord, String> {
    validate_request(&request)?;
    if is_process_running("UEVRInjector.exe") {
        return Err("Close UEVRInjector.exe before uninstalling its managed build.".to_owned());
    }
    let directory = backend_directory(&request.backend);
    if !marker_path(&request.backend).is_file() {
        return Err("No Moddin-managed UEVR installation was found for this backend.".to_owned());
    }

    let target_root = directory.to_string_lossy().into_owned();
    let record = transaction::list_transactions_sync()?
        .into_iter()
        .find(|record| {
            record.status == "applied"
                && record.kind == "uevr"
                && record.game_id == request.game_id
                && record.target_path == target_root
        })
        .ok_or_else(|| {
            "The managed UEVR marker exists, but its rollback transaction was not found.".to_owned()
        })?;
    transaction::rollback_transaction(record.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn request(backend: &str) -> UevrRequest {
        UevrRequest {
            game_id: "example".to_owned(),
            game_name: "Example".to_owned(),
            install_dir: r"C:\Games\Example".to_owned(),
            executable: "Game-Win64-Shipping.exe".to_owned(),
            backend: backend.to_owned(),
            version_policy: "latest".to_owned(),
            release_api_url: if backend == "afw" || backend == "joey-afw" {
                AFW_API.to_owned()
            } else {
                UEVR_NIGHTLY_API.to_owned()
            },
            release_tag: None,
            backend_release_api_url: (backend == "joey").then(|| JOEY_API.to_owned()),
            backend_release_tag: None,
            safety_notes: Vec::new(),
        }
    }

    #[test]
    fn accepts_official_backend_sources() {
        for backend in ["nightly", "joey", "afw", "joey-afw"] {
            assert!(validate_request(&request(backend)).is_ok(), "{backend}");
        }
    }

    #[test]
    fn rejects_unofficial_release_sources() {
        let mut value = request("nightly");
        value.release_api_url = "https://example.com/releases/latest".to_owned();
        assert!(validate_request(&value).is_err());
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        assert!(normalized_archive_path(r"..\outside.dll").is_err());
        assert!(normalized_archive_path(r"C:\outside.dll").is_err());
        assert_eq!(
            normalized_archive_path(r"UEVR\bin\UEVRInjector.exe").unwrap(),
            vec!["UEVR", "bin", "UEVRInjector.exe"]
        );
    }

    #[test]
    fn validates_pinned_tags() {
        let mut value = request("nightly");
        value.version_policy = "pinned".to_owned();
        value.release_tag = Some("nightly-01143-commit".to_owned());
        assert!(validate_request(&value).is_ok());
        value.release_tag = Some("../latest".to_owned());
        assert!(validate_request(&value).is_err());
    }

    #[test]
    fn strips_a_single_archive_root() {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("UEVR/UEVRInjector.exe", options).unwrap();
            writer.write_all(b"injector").unwrap();
            writer.start_file("UEVR/UEVRBackend.dll", options).unwrap();
            writer.write_all(b"backend").unwrap();
            writer.finish().unwrap();
        }

        let plan = archive_file_plan(&bytes.into_inner()).unwrap();
        assert!(plan
            .iter()
            .any(|file| file.relative_path == "UEVRInjector.exe"));
        assert!(plan
            .iter()
            .any(|file| file.relative_path == "UEVRBackend.dll"));
    }

    #[test]
    fn rejects_files_outside_the_archive_root() {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("UEVR/UEVRInjector.exe", options).unwrap();
            writer.write_all(b"injector").unwrap();
            writer.start_file("Other/readme.txt", options).unwrap();
            writer.write_all(b"unexpected").unwrap();
            writer.finish().unwrap();
        }

        assert!(archive_file_plan(&bytes.into_inner()).is_err());
    }
}
