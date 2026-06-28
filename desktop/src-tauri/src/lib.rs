mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::load_collections,
            commands::save_collections,
            commands::load_history,
            commands::save_history,
            commands::load_contexts,
            commands::save_contexts,
            commands::execute_request,
            commands::run_pre_script,
            commands::run_post_script,
            commands::interpolate_template,
            commands::run_collection_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error running hitop desktop app");
}
