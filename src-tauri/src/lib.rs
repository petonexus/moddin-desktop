mod activity;
mod cheeky;
mod desktop_shortcut;
mod inspection;
mod obs;
mod ofxr;
mod openxr;
mod optiscaler;
mod process;
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

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let url = validate_external_release_url(&url)?;
    let url = url.as_str();

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the release page: {error}"))
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the release page: {error}"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Could not open the release page: {error}"))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            activity::record_ui_action_log,
            activity::list_action_logs,
            activity::clear_action_logs,
            cheeky::preview_cheeky_foveated_dlss,
            cheeky::install_cheeky_foveated_dlss,
            cheeky::uninstall_cheeky_foveated_dlss,
            desktop_shortcut::preview_desktop_shortcut,
            desktop_shortcut::create_desktop_shortcut,
            steam::detect_steam_games,
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
            transaction::list_transactions,
            transaction::rollback_latest_module_transaction,
            transaction::rollback_transaction,
            updates::check_module_update,
            open_external_url,
            uevr::preview_uevr,
            uevr::install_uevr,
            uevr::uninstall_uevr,
            vr_launch::preview_vr_launch,
            vr_launch::launch_vr_game,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Moddin");
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
        assert!(validate_external_release_url(
            "https://github.com@evil.example/releases/tag/v1"
        )
        .is_err());
        assert!(validate_external_release_url(
            "https://user:pass@github.com/owner/repo/releases/tag/v1"
        )
        .is_err());
        assert!(validate_external_release_url("https://example.com/releases/tag/v1").is_err());
    }
}
