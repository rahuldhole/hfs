pub mod http;
pub mod network;

use std::sync::Mutex;
use tauri::State;
use tokio::sync::broadcast;

struct ServiceState {
    shutdown_tx: Mutex<Option<broadcast::Sender<()>>>,
}

#[tauri::command]
fn get_local_ips() -> Vec<String> {
    network::get_local_ips()
}

#[tauri::command]
async fn start_server_cmd(
    state: State<'_, ServiceState>,
    port: u16,
    shared_folders: Vec<String>,
) -> Result<(), String> {
    let (tx, rx) = broadcast::channel(1);
    
    // Stop existing server if any
    {
        let mut shutdown_tx = state.shutdown_tx.lock().unwrap();
        if let Some(tx) = shutdown_tx.take() {
            let _ = tx.send(());
        }
        *shutdown_tx = Some(tx);
    }

    // Spawn server task
    tauri::async_runtime::spawn(async move {
        if let Err(e) = http::start_server(port, shared_folders, rx).await {
            eprintln!("Server error: {}", e);
        }
    });

    Ok(())
}

#[tauri::command]
async fn stop_server_cmd(state: State<'_, ServiceState>) -> Result<(), String> {
    let mut shutdown_tx = state.shutdown_tx.lock().unwrap();
    if let Some(tx) = shutdown_tx.take() {
        let _ = tx.send(());
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(ServiceState {
            shutdown_tx: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_local_ips,
            start_server_cmd,
            stop_server_cmd
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
