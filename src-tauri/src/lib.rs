mod activity;
mod ai_agent_runner;
mod ai_assistant_setup;
mod app_update;
mod archive;
mod builtin_checks;
mod builtin_steps;
mod capability;
mod capability_authoring;
mod capability_runner;
mod cheeky;
mod collection;
mod community_catalog;
mod desktop_shortcut;
mod gog;
mod inspection;
mod module;
mod obs;
mod ofxr;
mod openxr;
mod optiscaler;
mod path_guard;
mod pcgw_cache;
mod process;
mod profile_files;
mod steam;
mod transaction;
mod uevr;
mod updates;
mod vr_launch;

fn validate_external_release_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value.trim())
        .map_err(|_| "Release link is not a valid URL.".to_owned())?;

    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("External release links must use HTTPS on github.com.".to_owned());
    }

    Ok(url)
}

fn open_url_in_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the link: {error}"))
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the link: {error}"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the link: {error}"))
    }
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let url = validate_external_release_url(&url)?;
    let url = url.as_str();

    open_url_in_browser(url)
}

/// Open any HTTPS link in the user's default browser. Used for AI
/// assistant deep-links (ChatGPT / Claude / Gemini), which
/// `open_external_url` deliberately rejects (it is release-page
/// specific and github.com-only).
#[tauri::command]
fn open_web_url(url: String) -> Result<(), String> {
    let url = reqwest::Url::parse(url.trim()).map_err(|_| "Link is not a valid URL.".to_owned())?;
    if url.scheme() != "https" {
        return Err("Only HTTPS links can be opened.".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Links with embedded credentials are not allowed.".to_owned());
    }

    open_url_in_browser(url.as_str())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // The updater plugin owns the endpoint, the pubkey and the
        // minisign check; `app_update` is the only thing the webview can
        // ask it to do, and the capability set grants the plugin's own
        // commands to nobody. See src-tauri/capabilities/app-update.yaml.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // The file dialog, for the same reason and with the same shape:
        // `profile_files` opens the two dialogs, and
        // src-tauri/capabilities/dialog.json grants the webview no
        // permission on the plugin. A dialog the frontend could raise on
        // its own would let it read a path the user never chose and write
        // to one they did.
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            activity::record_ui_action_log,
            app_update::app_update_status,
            app_update::app_update_check,
            app_update::app_update_install,
            capability_runner::capability_install,
            capability_runner::capability_uninstall,
            capability_runner::capability_evaluate,
            capability_runner::capability_compatibility,
            capability_runner::capability_list,
            capability_runner::capability_get,
            capability_runner::capability_reload,
            capability_runner::community_capability_install,
            // The one collection command. `collection_install`,
            // `collection_resume`, `collection_abort` and
            // `collection_save_yaml` belonged to the session runner the
            // `feat/agent-mcp` branch paired this loader with; they are
            // not here, and registering them would put a second install
            // path back next to `community_capability_install`.
            collection::collection_list,
            capability_authoring::validate_capability_yaml,
            capability_authoring::preview_capability_plan,
            capability_authoring::save_capability_yaml,
            capability_authoring::validate_recommendations_yaml,
            profile_files::save_profile_file,
            profile_files::pick_profile_file,
            profile_files::read_profile_file,
            profile_files::reveal_profile_file,
            community_catalog::community_catalog_fetch,
            community_catalog::community_catalog_set_ttl,
            activity::list_action_logs,
            activity::clear_action_logs,
            cheeky::preview_cheeky_foveated_dlss,
            cheeky::install_cheeky_foveated_dlss,
            cheeky::uninstall_cheeky_foveated_dlss,
            desktop_shortcut::preview_desktop_shortcut,
            desktop_shortcut::create_desktop_shortcut,
            steam::detect_installed_games,
            inspection::inspect_game_environment,
            ofxr::preview_ofxr,
            ofxr::install_ofxr,
            ofxr::uninstall_ofxr,
            obs::preview_obs_vr,
            obs::configure_obs_vr,
            obs::uninstall_obs_vr,
            openxr::inspect_openxr,
            openxr::set_game_openxr_runtime,
            openxr::set_system_openxr_runtime,
            optiscaler::preview_optiscaler,
            optiscaler::install_optiscaler,
            optiscaler::uninstall_optiscaler,
            // UE4SS, BepInEx, REFramework and ReShade no longer expose
            // dedicated commands: they are capability specs
            // (src-tauri/capabilities/), so they install through
            // `capability_install` with the same checks, preview and
            // rollback as every other recipe.
            transaction::list_transactions,
            transaction::rollback_latest_module_transaction,
            transaction::rollback_transaction,
            transaction::create_snapshot,
            transaction::list_snapshots,
            transaction::rollback_snapshot,
            transaction::delete_snapshot,
            updates::check_module_update,
            open_external_url,
            open_web_url,
            uevr::preview_uevr,
            uevr::install_uevr,
            uevr::uninstall_uevr,
            vr_launch::preview_vr_launch,
            vr_launch::launch_vr_game,
            pcgw_cache::lookup_pcgw_summary,
            pcgw_cache::get_pcgw_cache,
            pcgw_cache::clear_pcgw_cache,
            ai_assistant_setup::detect_ai_assistants,
            ai_assistant_setup::setup_ai_assistant,
            ai_assistant_setup::remove_ai_assistant,
            ai_agent_runner::list_agent_clis,
            ai_agent_runner::run_ai_agent_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Moddin");
}

#[cfg(test)]
pub mod test_support {
    /// Process-wide lock for tests that read or mutate process env vars
    /// (LOCALAPPDATA / USERPROFILE). Env-mutating tests in
    /// `ai_assistant_setup` hold this for their whole body; tests that
    /// read env-derived paths (like the compat marker test) must hold it
    /// too, or a concurrent env test's temp dir can vanish mid-read.
    pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        // Poisoning is recovered from on purpose. A failing test that
        // held the lock used to make every other env-touching test fail
        // with "env test lock", hiding the one real failure behind a
        // dozen fake ones.
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_github_https_release_links() {
        let url = validate_external_release_url(
            "https://github.com/optiscaler/OptiScaler/releases/tag/v0.9.4",
        )
        .expect("GitHub release URL should be accepted");
        assert_eq!(url.host_str(), Some("github.com"));
    }

    #[test]
    fn rejects_http_credentials_and_other_hosts() {
        assert!(validate_external_release_url(
            "http://github.com/optiscaler/OptiScaler/releases/tag/v0.9.4"
        )
        .is_err());
        assert!(
            validate_external_release_url("https://github.com@evil.example/releases/tag/v1")
                .is_err()
        );
        assert!(validate_external_release_url(
            "https://user:pass@github.com/owner/repo/releases/tag/v1"
        )
        .is_err());
        assert!(validate_external_release_url("https://example.com/releases/tag/v1").is_err());
    }

    #[test]
    fn open_web_url_rejects_non_https_and_malformed_links() {
        // Only the validation paths are exercised — a valid link would
        // spawn a real browser.
        assert!(open_web_url("http://chatgpt.com/".to_owned()).is_err());
        assert!(open_web_url("not a url".to_owned()).is_err());
        assert!(open_web_url("https://user:pass@chatgpt.com/".to_owned()).is_err());
    }
}
