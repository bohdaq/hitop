mod bookmarks;
mod commands;
mod git;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::load_collections,
            commands::save_collections,
            commands::load_history,
            commands::save_history,
            commands::load_contexts,
            commands::save_contexts,
            commands::load_environments,
            commands::save_environments,
            commands::load_globals,
            commands::save_globals,
            commands::execute_request,
            commands::run_pre_script,
            commands::run_post_script,
            commands::interpolate_template,
            commands::run_collection_cmd,
            commands::send_request,
            commands::send_cancel,
            commands::cookies_load,
            commands::cookies_save,
            commands::read_folder,
            commands::write_folder,
            commands::secret_get,
            commands::secret_set,
            commands::secret_delete,
            commands::secret_refs_resolve,
            commands::secret_manager_settings,
            commands::secret_manager_settings_set,
            commands::write_file,
            commands::telemetry_status,
            commands::telemetry_set,
            commands::ws_connect,
            commands::ws_send,
            commands::ws_close,
            commands::grpc_list,
            commands::grpc_describe,
            commands::grpc_call,
            commands::grpc_cancel,
            bookmarks::folder_forget,
            git::git,
            git::git_token_status,
            git::git_token_set,
            git::git_token_delete,
        ])
        .manage(commands::Sockets::default())
        .manage(commands::GrpcCalls::default())
        .manage(commands::HttpCalls::default())
        .setup(|_app| {
            // The usage ping, when the user turned it on (off by default).
            #[cfg(feature = "telemetry")]
            commands::ping_in_background(_app.package_info().version.to_string());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running hitop desktop app");
}
