mod inspection;
mod obs;
mod optiscaler;
mod steam;
mod transaction;
mod vr_launch;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            steam::detect_steam_games,
            inspection::inspect_game_environment,
            obs::preview_obs_vr,
            obs::configure_obs_vr,
            optiscaler::preview_optiscaler,
            optiscaler::install_optiscaler,
            transaction::list_transactions,
            transaction::rollback_transaction,
            vr_launch::preview_vr_launch,
            vr_launch::launch_vr_game,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Moddin");
}
