mod activity;
mod inspection;
mod obs;
mod openxr;
mod optiscaler;
mod steam;
mod transaction;
mod updates;
mod vr_launch;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            activity::record_ui_action_log,
            activity::list_action_logs,
            activity::clear_action_logs,
            steam::detect_steam_games,
            inspection::inspect_game_environment,
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
            vr_launch::preview_vr_launch,
            vr_launch::launch_vr_game,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Moddin");
}
