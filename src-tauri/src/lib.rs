mod docmind;

use tauri_plugin_dialog::init as dialog_init;
use tauri_plugin_opener::init as opener_init;

pub fn run() {
    let database = tauri::async_runtime::block_on(docmind::storage::Database::open_or_init())
        .expect("failed to initialize DocMind SQLite database");

    tauri::Builder::default()
        .manage(database)
        .plugin(dialog_init())
        .plugin(opener_init())
        .invoke_handler(tauri::generate_handler![
            crate::docmind::commands::list_index_dirs,
            crate::docmind::commands::search_documents,
            crate::docmind::commands::get_index_status,
            crate::docmind::commands::open_file,
            crate::docmind::commands::refresh_index,
            crate::docmind::commands::refresh_index_dir,
            crate::docmind::commands::add_index_dir,
            crate::docmind::commands::remove_index_dir,
            crate::docmind::commands::set_index_dir_enabled,
            crate::docmind::commands::retry_failed_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocMind application");
}
