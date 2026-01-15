use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::fs::File;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::io::ReaderStream;
use tokio_util::compat::FuturesAsyncWriteCompatExt;


#[derive(Clone, Serialize, Deserialize)]
pub struct ServerState {
    pub shared_folders: Vec<String>,
    pub port: u16,
    pub is_running: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub shared_folders: Arc<Mutex<Vec<String>>>,
}

pub async fn start_server(
    port: u16,
    shared_folders: Vec<String>,
    mut shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), String> {
    let state = AppState {
        shared_folders: Arc::new(Mutex::new(shared_folders)),
    };

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/download/*path", get(file_handler))
        .route("/zip/folder/*path", get(zip_folder_handler))
        .route("/zip/selection", axum::routing::post(zip_selection_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.map_err(|e| e.to_string())?;

    println!("Server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_rx.recv().await.ok();
            println!("Server shutting down");
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// Helper to resolve a relative URL path to a real file path based on shared items
fn resolve_path(shared_items: &[String], relative_path: &str) -> Option<PathBuf> {
    let relative_path = relative_path.trim_start_matches('/');
    
    // Iterate over shared items
    for item in shared_items {
        let item_path = PathBuf::from(item);
        let item_name = item_path.file_name()?.to_string_lossy();
        
        if relative_path == item_name {
            return Some(item_path);
        } else if relative_path.starts_with(&format!("{}/", item_name)) {
            let rest = &relative_path[item_name.len() + 1..];
            return Some(item_path.join(rest));
        }
    }
    None
}

async fn file_handler(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    if path.contains("..") {
        return Err((StatusCode::FORBIDDEN, "Invalid path".to_string()));
    }

    let file_path = {
        let folders = state.shared_folders.lock().unwrap();
        resolve_path(&folders, &path)
    };
    
    let file_path = file_path.ok_or((StatusCode::NOT_FOUND, "File not found".to_string()))?;

    if !file_path.exists() || file_path.is_dir() {
         return Err((StatusCode::NOT_FOUND, "File not found".to_string()));
    }

    let file = File::open(&file_path).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let filename = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
    
    Ok(Response::builder()
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename))
        .body(body)
        .unwrap())
}

use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use tokio::io::duplex;

async fn zip_folder_handler(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Response, (StatusCode, String)> {
     if path.contains("..") {
        return Err((StatusCode::FORBIDDEN, "Invalid path".to_string()));
    }

    let target_path = {
        let folders = state.shared_folders.lock().unwrap();
        resolve_path(&folders, &path)
    };
    
    let target_path = target_path.ok_or((StatusCode::NOT_FOUND, "Path not found".to_string()))?;

     if !target_path.exists() || !target_path.is_dir() {
         return Err((StatusCode::NOT_FOUND, "Folder not found".to_string()));
    }

    let (w, r) = duplex(64 * 1024);
    let stream = ReaderStream::new(r);
    let body = Body::from_stream(stream);

    let target_path_clone = target_path.clone();
    let parent_path = target_path.parent().unwrap_or(&target_path).to_path_buf();

    tokio::spawn(async move {
        let mut writer = ZipFileWriter::with_tokio(w);
        let mut stack = vec![target_path_clone];
        
        while let Some(current_dir) = stack.pop() {
            let mut entries = match tokio::fs::read_dir(&current_dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path.clone());
                    // Ideally we add empty dirs too, but for file download mostly files matter
                } else {
                    let relative_path = path.strip_prefix(&parent_path).unwrap_or(&path);
                    let filename = relative_path.to_string_lossy().into_owned();
                    
                    let builder = ZipEntryBuilder::new(filename.into(), Compression::Deflate);
                    
                    if let Ok(mut file) = File::open(&path).await {
                         if let Ok(entry_writer) = writer.write_entry_stream(builder).await {
                             let mut compat_writer = entry_writer.compat_write();
                             let _ = tokio::io::copy(&mut file, &mut compat_writer).await;
                             let _ = compat_writer.into_inner().close().await;
                         }
                    }
                }
            }
        }
        let _ = writer.close().await;
    });

    let zip_name = target_path.file_name().unwrap_or_default().to_string_lossy().to_string();

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}.zip\"", zip_name))
        .body(body)
        .unwrap())
}

#[derive(Deserialize)]
struct SelectionRequest {
    files: Vec<String>,
}

async fn zip_selection_handler(
    State(state): State<AppState>,
    Json(payload): Json<SelectionRequest>,
) -> Result<Response, (StatusCode, String)> {
    let (w, r) = duplex(64 * 1024);
    let stream = ReaderStream::new(r);
    let body = Body::from_stream(stream);
    
    let shared_folders = state.shared_folders.lock().unwrap().clone();
    
    tokio::spawn(async move {
        let mut writer = ZipFileWriter::with_tokio(w);
        
        for rel_path in payload.files {
           if rel_path.contains("..") { continue; }
           
           if let Some(full_path) = resolve_path(&shared_folders, &rel_path) {
                if full_path.is_file() {
                    let filename = rel_path.clone(); // Use the relative path requested as name in zip
                    let builder = ZipEntryBuilder::new(filename.into(), Compression::Deflate);
                    
                    if let Ok(mut file) = File::open(&full_path).await {
                         if let Ok(entry_writer) = writer.write_entry_stream(builder).await {
                             let mut compat_writer = entry_writer.compat_write();
                             let _ = tokio::io::copy(&mut file, &mut compat_writer).await;
                             let _ = compat_writer.into_inner().close().await;
                         }
                    }
                } else if full_path.is_dir() {
                    // Start recursive add
                     let parent_path = full_path.parent().unwrap_or(&full_path).to_path_buf();
                      let mut stack = vec![full_path.clone()];
                        while let Some(current_dir) = stack.pop() {
                            let mut entries = match tokio::fs::read_dir(&current_dir).await {
                                Ok(e) => e,
                                Err(_) => continue,
                            };
                            while let Ok(Some(entry)) = entries.next_entry().await {
                                let path = entry.path();
                                if path.is_dir() {
                                    stack.push(path.clone());
                                } else {
                                    // Make relative to the PARENT of the selected folder so "FolderA/file" structure is kept?
                                    // Or relative to the root of zip?
                                    // If I selected "FolderA", I expect "FolderA/file" in zip.
                                    // "FolderA" path in `resolve_path` is based on shared root.
                                    // We need to map `full_path` -> `rel_path` + (path - full_path).
                                    // This is tricky. Simplified: Just flatten or use relative to shared root?
                                    // Just using keys provided in request seems reasonably safe if we resolve them.
                                    // But we need to recurse.
                                    // Let's stick to files for selection MVP or simple recursion.
                                    // Actually, if we support picking folders in "Selection", we need deep zip.
                                    
                                    // Better approach for selection: Just zip file-by-file.
                                    // If folder is in selection, we walk it.
                                    // Relative path in ZIP should be `rel_path` / `subpath`.
                                    let sub_rel = path.strip_prefix(&full_path).unwrap_or(&path);
                                    let zip_entry_name = format!("{}/{}", rel_path, sub_rel.to_string_lossy());

                                    let builder = ZipEntryBuilder::new(zip_entry_name.into(), Compression::Deflate);
                                     if let Ok(mut file) = File::open(&path).await {
                                         if let Ok(entry_writer) = writer.write_entry_stream(builder).await {
                                             let mut compat_writer = entry_writer.compat_write();
                                             let _ = tokio::io::copy(&mut file, &mut compat_writer).await;
                                             let _ = compat_writer.into_inner().close().await;
                                         }
                                    }
                                }
                            }
                        }
                }
           }
        }
        
        let _ = writer.close().await;
    });

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"download.zip\"")
        .body(body)
        .unwrap())
}

async fn root_handler(State(state): State<AppState>) -> Html<String> {
    let folders = state.shared_folders.lock().unwrap();
    
    // Generate simple HTML list
    let mut list_html = String::new();
    for folder in folders.iter() {
        let path = PathBuf::from(folder);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        // If it's a file
        if path.is_file() {
             list_html.push_str(&format!(r#"<li><a href="/download/{}">{}</a></li>"#, name, name));
        } else {
             // It's a directory
             list_html.push_str(&format!(
                 r#"<li>
                    <strong>{}</strong> 
                    <a href="/zip/folder/{}">[Download Zip]</a>
                 </li>"#, 
                 name, name));
             // We should also probably list children?
             // Since "Root endpoint serves file browser UI", we should probably READ the directory and list children.
             // But if we shared multiple folders, we just list the roots first?
             // Yes, top level is roots.
        }
    }

    Html(format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>HFS Share</title>
            <style>
                body {{ font-family: sans-serif; max-width: 800px; margin: 2rem auto; padding: 1rem; }}
                li {{ margin: 0.5rem 0; }}
                a {{ text-decoration: none; color: #0066cc; }}
            </style>
        </head>
        <body>
            <h1>HFS File Share</h1>
            <p>Shared Folders:</p>
            <ul>
                {}
            </ul>
        </body>
        </html>
        "#,
        list_html
    ))
}

