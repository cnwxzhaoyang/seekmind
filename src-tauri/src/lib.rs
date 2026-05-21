mod docmind;

use tauri_plugin_opener::init as opener_init;

pub fn run() {
    tauri::Builder::default()
        .plugin(opener_init())
        .invoke_handler(tauri::generate_handler![
            crate::docmind::commands::list_index_dirs,
            crate::docmind::commands::search_documents,
            crate::docmind::commands::get_index_status,
            crate::docmind::commands::open_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocMind application");
}
