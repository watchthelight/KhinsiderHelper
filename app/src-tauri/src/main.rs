#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod download;
mod models;
mod scraper;
mod state;

use state::KhinsiderState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(KhinsiderState::default())
        .invoke_handler(tauri::generate_handler![
            auth::try_restore_session,
            auth::login,
            auth::check_auth,
            auth::fetch_library,
            scraper::search_albums,
            scraper::fetch_album_detail,
            scraper::resolve_song_links,
            download::start_album_download,
            download::start_library_download,
            download::check_local_albums,
            download::cancel_downloads,
            download::get_default_output_directory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
