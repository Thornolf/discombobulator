#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use discombobulator_server::{AppState, DEFAULT_PORT, local_address};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const MAIN_WINDOW: &str = "main";
const MINIMIZED_FLAG: &str = "--minimized";
const AUTOSTART_CONFIGURED: &str = "autostart_configured";

type AnyError = Box<dyn std::error::Error + Send + Sync>;

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let url = format!("http://{}/", local_address(DEFAULT_PORT));
    let Ok(url) = url.parse() else { return };
    let _ = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::External(url))
        .title("Discombobulator")
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 600.0)
        .maximized(true)
        .build();
}

fn start_server(state: Arc<AppState>) -> Result<(), AnyError> {
    let listener = std::net::TcpListener::bind(local_address(DEFAULT_PORT))?;
    listener.set_nonblocking(true)?;
    tauri::async_runtime::spawn(async move {
        let opener = Arc::clone(&state);
        let _ = tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            let _guard = runtime.enter();
            opener.reopen_last_folder();
            opener.refresh_tailnet_host();
        })
        .await;
        if let Ok(listener) = tokio::net::TcpListener::from_std(listener) {
            let _ = discombobulator_server::serve(state, listener).await;
        }
    });
    Ok(())
}

fn configure_autostart(app: &AppHandle, state: &AppState) {
    if state
        .store
        .setting(AUTOSTART_CONFIGURED)
        .ok()
        .flatten()
        .is_some()
    {
        return;
    }
    if app.autolaunch().enable().is_ok() {
        let _ = state.store.set_setting(AUTOSTART_CONFIGURED, "1");
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Discombobulator", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with Windows",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &autostart, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Discombobulator")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_window(app),
            "autostart" => {
                let launcher = app.autolaunch();
                let enabled = launcher.is_enabled().unwrap_or(false);
                let _ = if enabled {
                    launcher.disable()
                } else {
                    launcher.enable()
                };
                let _ = autostart.set_checked(launcher.is_enabled().unwrap_or(false));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    let data_dir = discombobulator_server::default_data_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("discombobulator"));
    let state = match AppState::open(&data_dir, DEFAULT_PORT) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("cannot open the photo index: {error}");
            std::process::exit(1);
        }
    };
    let minimized = std::env::args().any(|arg| arg == MINIMIZED_FLAG);
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_FLAG]),
        ))
        .setup(move |app| {
            start_server(Arc::clone(&state)).map_err(|error| error.to_string())?;
            configure_autostart(app.handle(), &state);
            build_tray(app.handle())?;
            if !minimized {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|_, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event
                && code.is_none()
            {
                api.prevent_exit();
            }
        }),
        Err(error) => {
            eprintln!("cannot start the window: {error}");
            std::process::exit(1);
        }
    }
}
