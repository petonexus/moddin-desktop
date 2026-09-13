mod activity;
mod cheeky;
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

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only HTTP and HTTPS URLs can be opened externally.".to_owned());
    }

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
