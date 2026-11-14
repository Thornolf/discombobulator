#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            panic!(
                "{} failed to start: {error}",
                discombobulator_core::APP_NAME
            )
        });
}
