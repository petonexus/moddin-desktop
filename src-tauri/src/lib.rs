mod steam;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![steam::detect_steam_games])
        .run(tauri::generate_context!())
        .expect("error while running Moddin");
}
