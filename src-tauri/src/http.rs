use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, Response, IntoResponse},
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
        .route("/api/browse", get(browse_handler))
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
    let relative_path = relative_path.trim_matches('/');
    if relative_path.is_empty() { return None; }

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

#[derive(Serialize)]
struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: Option<u64>,
}

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

#[axum::debug_handler]
async fn browse_handler(
    State(state): State<AppState>,
    Query(query): Query<BrowseQuery>,
) ->  impl IntoResponse {
    let req_path = query.path.unwrap_or_else(|| "/".to_string());
    let req_path_clean = req_path.trim_matches('/');

    let mut entries = Vec::new();

    if req_path_clean.is_empty() {
        // Root: list shared folders
        // We hold lock only here, no awaits
        let folders = state.shared_folders.lock().unwrap();
        for folder in folders.iter() {
            let path = PathBuf::from(folder);
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy().to_string();
                entries.push(FileEntry {
                    name: name_str.clone(),
                    path: name_str,
                    is_dir: path.is_dir(),
                    size: if path.is_file() { path.metadata().ok().map(|m| m.len()) } else { None },
                });
            }
        }
    } else {
        // Subpath
        // Resolve path inside lock, then drop lock
        let real_path = {
            let folders = state.shared_folders.lock().unwrap();
            resolve_path(&folders, req_path_clean)
        };

        if let Some(real_path) = real_path {
            if let Ok(mut dir) = tokio::fs::read_dir(real_path).await {
                while let Ok(Some(entry)) = dir.next_entry().await {
                   let name = entry.file_name().to_string_lossy().to_string();
                   // Skip hidden files
                   if name.starts_with('.') { continue; }
                   let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                   let size = if !is_dir { entry.metadata().await.ok().map(|m| m.len()) } else { None };
                   
                   entries.push(FileEntry {
                       name: name.clone(),
                       path: format!("{}/{}", req_path_clean, name),
                       is_dir,
                       size,
                   });
                }
            }
        }
    }
    
    // Sort directories first, then files
    entries.sort_by(|a, b| {
        if a.is_dir == b.is_dir {
            a.name.cmp(&b.name)
        } else if a.is_dir {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });

    Json(entries)
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

async fn root_handler() -> Html<&'static str> {
    Html(r##"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>HFS Client</title>
    <link rel="icon" type="image/svg+xml" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 512 512'%3E%3Cdefs%3E%3ClinearGradient id='bg' x1='0%25' y1='0%25' x2='100%25' y2='100%25'%3E%3Cstop offset='0%25' style='stop-color:%233B82F6'/%3E%3Cstop offset='100%25' style='stop-color:%234F46E5'/%3E%3C/linearGradient%3E%3C/defs%3E%3Crect width='512' height='512' rx='96' fill='url(%23bg)'/%3E%3Cg fill='none' stroke='white' stroke-width='24' stroke-linecap='round' stroke-linejoin='round'%3E%3Ccircle cx='256' cy='172' r='40' fill='white'/%3E%3Ccircle cx='160' cy='340' r='40' fill='white'/%3E%3Ccircle cx='352' cy='340' r='40' fill='white'/%3E%3Cline x1='256' y1='212' x2='180' y2='305'/%3E%3Cline x1='256' y1='212' x2='332' y2='305'/%3E%3Cline x1='200' y1='340' x2='312' y2='340'/%3E%3C/g%3E%3C/svg%3E">
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
    <script src="https://cdn.tailwindcss.com"></script>
    <script src="https://unpkg.com/vue@3/dist/vue.global.js"></script>
    <script src="https://unpkg.com/lucide@latest"></script>
    <script>
        tailwind.config = {
            theme: {
                extend: {
                    fontFamily: { sans: ['Inter', 'system-ui', 'sans-serif'] }
                }
            }
        }
    </script>
    <style>
        [v-cloak] { display: none !important; }
        .scrollbar-hide { -ms-overflow-style: none; scrollbar-width: none; }
        .scrollbar-hide::-webkit-scrollbar { display: none; }
        ::-webkit-scrollbar { width: 8px; height: 8px; }
        ::-webkit-scrollbar-track { background: transparent; }
        ::-webkit-scrollbar-thumb { background: #27272a; border-radius: 4px; }
        ::-webkit-scrollbar-thumb:hover { background: #3f3f46; }
    </style>
</head>
<body class="bg-zinc-950 text-zinc-200 font-sans min-h-screen flex flex-col">
    <div id="app" v-cloak class="min-h-screen flex flex-col">
        <!-- Header -->
        <header class="bg-zinc-900/80 backdrop-blur-xl border-b border-zinc-800 sticky top-0 z-20">
            <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 h-16 flex items-center justify-between">
                <div class="flex items-center gap-3 sm:gap-4">
                    <div class="relative">
                        <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-blue-500 to-indigo-600 flex items-center justify-center shadow-lg shadow-blue-500/30">
                           <i data-lucide="network" class="w-5 h-5 text-white"></i>
                        </div>
                        <span class="absolute -bottom-0.5 -right-0.5 w-3 h-3 bg-emerald-500 border-2 border-zinc-900 rounded-full"></span>
                    </div>
                    <div>
                        <h1 class="text-lg sm:text-xl font-bold text-white">HFS Client</h1>
                        <p class="text-[10px] text-zinc-500 hidden sm:block">Secure Local File Transfer</p>
                    </div>
                </div>
                
                <div class="flex items-center gap-2 sm:gap-3">
                     <span class="text-[10px] sm:text-xs font-medium bg-emerald-500/10 text-emerald-400 px-2 sm:px-3 py-1 rounded-full border border-emerald-500/30 flex items-center gap-1.5">
                        <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                        <span class="hidden sm:inline">Connected</span>
                     </span>
                </div>
            </div>
        </header>

        <!-- Main Area -->
        <main class="flex-1 flex flex-col max-w-7xl mx-auto w-full px-4 sm:px-6 lg:px-8 py-4 sm:py-6">
            
            <!-- Toolbar Card -->
            <div class="bg-zinc-900/50 rounded-t-2xl border border-zinc-800 p-2 sm:p-3 flex flex-wrap items-center gap-2 justify-between backdrop-blur-sm">
               <!-- Breadcrumbs -->
               <div class="flex items-center gap-0.5 sm:gap-1 text-sm overflow-x-auto scrollbar-hide flex-1 min-w-0">
                  <button @click="navigate('/')" class="p-1.5 sm:p-2 hover:bg-zinc-800 rounded-lg text-zinc-500 hover:text-blue-400 transition-all active:scale-95 shrink-0">
                      <i data-lucide="home" class="w-4 h-4"></i>
                  </button>
                  <template v-for="(part, index) in breadcrumbs" :key="index">
                      <i data-lucide="chevron-right" class="w-4 h-4 text-zinc-600 shrink-0"></i>
                      <button @click="navigate(part.path)" class="px-2 sm:px-3 py-1 rounded-lg hover:bg-zinc-800 text-zinc-300 font-medium hover:text-blue-400 transition-all whitespace-nowrap truncate max-w-[120px] sm:max-w-none active:scale-95">
                          {{ part.name }}
                      </button>
                  </template>
               </div>

               <!-- View Actions -->
               <div class="flex items-center gap-2 shrink-0">
                   <div class="flex bg-zinc-800 rounded-xl p-1 border border-zinc-700">
                        <button @click="viewMode = 'grid'" :class="{'bg-zinc-700 shadow-sm text-blue-400': viewMode === 'grid', 'text-zinc-500 hover:text-zinc-300': viewMode !== 'grid'}" class="p-2 rounded-lg transition-all active:scale-95">
                            <i data-lucide="layout-grid" class="w-4 h-4"></i>
                        </button>
                        <button @click="viewMode = 'list'" :class="{'bg-zinc-700 shadow-sm text-blue-400': viewMode === 'list', 'text-zinc-500 hover:text-zinc-300': viewMode !== 'list'}" class="p-2 rounded-lg transition-all active:scale-95">
                            <i data-lucide="list" class="w-4 h-4"></i>
                        </button>
                   </div>
               </div>
            </div>

            <!-- Selection Toolbar -->
            <div v-if="selectedItems.length > 0" class="bg-blue-500/10 border-x border-blue-500/20 px-4 py-3 flex flex-wrap items-center justify-between gap-3">
                 <div class="text-sm text-blue-400 font-medium flex items-center gap-2">
                     <div class="w-6 h-6 rounded-full bg-blue-500/20 flex items-center justify-center">
                        <i data-lucide="check" class="w-3.5 h-3.5 text-blue-400"></i>
                     </div>
                     {{ selectedItems.length }} item{{ selectedItems.length > 1 ? 's' : '' }} selected
                 </div>
                 <div class="flex gap-2">
                     <button @click="clearSelection" class="px-3 py-1.5 rounded-lg text-sm font-medium text-zinc-400 hover:bg-zinc-800 transition-colors active:scale-95">
                         Clear
                     </button>
                     <button @click="downloadSelection" class="bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-500 hover:to-indigo-500 text-white px-4 py-1.5 rounded-lg text-sm font-semibold flex items-center gap-2 transition-all shadow-lg shadow-blue-500/25 active:scale-95">
                         <i data-lucide="download" class="w-4 h-4"></i>
                         <span class="hidden sm:inline">Download as Zip</span>
                         <span class="sm:hidden">Download</span>
                     </button>
                 </div>
            </div>
            <div v-else class="h-px bg-zinc-800"></div>

            <!-- Content Area -->
            <div class="flex-1 bg-zinc-900/30 border-x border-b border-zinc-800 rounded-b-2xl overflow-hidden flex flex-col backdrop-blur-sm">
                <div class="flex-1 overflow-y-auto p-4 sm:p-6" @click.self="clearSelection">
                    
                    <!-- Loading -->
                    <div v-if="loading" class="h-64 flex flex-col items-center justify-center">
                        <div class="w-12 h-12 rounded-full border-4 border-zinc-700 border-t-blue-500 animate-spin mb-4"></div>
                        <p class="text-zinc-500 text-sm">Loading files...</p>
                    </div>

                    <!-- Empty -->
                    <div v-else-if="items.length === 0" class="h-64 flex flex-col items-center justify-center text-zinc-500">
                        <div class="w-20 h-20 rounded-full bg-zinc-800 flex items-center justify-center mb-4">
                            <i data-lucide="folder-open" class="w-10 h-10 text-zinc-600"></i>
                        </div>
                        <p class="font-medium text-zinc-400">This folder is empty</p>
                        <p class="text-sm mt-1">No files or folders to display</p>
                    </div>

                    <!-- Grid View -->
                    <div v-else-if="viewMode === 'grid'" class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-3 sm:gap-4">
                        <div v-for="item in items" :key="item.path" 
                             @click.exact="toggleSelect(item)"
                             @dblclick="handleOpen(item)"
                             :class="{'ring-2 ring-blue-500 bg-blue-500/10': isSelected(item), 'hover:bg-zinc-800/50': !isSelected(item)}"
                             class="group relative p-3 sm:p-4 rounded-xl border border-zinc-800 cursor-pointer transition-all duration-200 flex flex-col items-center text-center select-none hover:border-zinc-700">
                            
                            <!-- Checkbox -->
                            <div class="absolute top-2 left-2 z-10 transition-all" :class="{'opacity-100 scale-100': isSelected(item), 'opacity-0 group-hover:opacity-100 scale-90 group-hover:scale-100': !isSelected(item)}">
                                <div :class="isSelected(item) ? 'bg-blue-600 border-blue-600' : 'bg-zinc-800 border-zinc-600'" class="w-5 h-5 rounded-md border-2 flex items-center justify-center transition-colors">
                                    <i v-if="isSelected(item)" data-lucide="check" class="w-3 h-3 text-white"></i>
                                </div>
                            </div>

                            <!-- Download Quick Action -->
                            <button @click.stop="downloadItem(item)" class="absolute top-2 right-2 w-8 h-8 bg-zinc-800/90 backdrop-blur rounded-lg border border-zinc-700 flex items-center justify-center text-zinc-400 hover:text-blue-400 hover:border-blue-500/50 opacity-0 group-hover:opacity-100 transition-all z-10 active:scale-95">
                                <i data-lucide="download" class="w-4 h-4"></i>
                            </button>

                            <!-- Icon -->
                            <div class="mb-3 transition-transform duration-200 group-hover:scale-105">
                                <div v-if="item.is_dir" class="w-14 h-14 sm:w-16 sm:h-16 flex items-center justify-center">
                                    <i data-lucide="folder" class="w-14 h-14 sm:w-16 sm:h-16 text-amber-400 fill-amber-400/20"></i>
                                </div>
                                <div v-else class="w-12 h-14 sm:w-14 sm:h-16 relative flex items-center justify-center">
                                    <i data-lucide="file" class="w-12 h-14 sm:w-14 sm:h-16 text-zinc-500"></i>
                                    <span class="absolute bottom-3 text-[8px] sm:text-[9px] font-bold text-zinc-400 uppercase">{{ getExt(item.name) }}</span>
                                </div>
                            </div>
                            
                            <!-- Name & Size -->
                            <div class="text-xs sm:text-sm font-medium text-zinc-300 truncate w-full px-1" :title="item.name">{{ item.name }}</div>
                            <div class="text-[10px] sm:text-xs text-zinc-500 mt-1">{{ formatSize(item.size) }}</div>
                        </div>
                    </div>

                    <!-- List View -->
                    <div v-else class="flex flex-col -mx-2 sm:mx-0">
                        <!-- Header -->
                        <div class="hidden sm:grid grid-cols-12 gap-4 px-4 py-2 text-xs font-semibold text-zinc-500 border-b border-zinc-800 uppercase tracking-wider sticky top-0 bg-zinc-900/90 backdrop-blur z-10">
                            <div class="col-span-1"></div>
                            <div class="col-span-6">Name</div>
                            <div class="col-span-2 text-right">Size</div>
                            <div class="col-span-3 text-right">Actions</div>
                        </div>
                        
                        <!-- Items -->
                        <div v-for="item in items" :key="item.path"
                             @click.exact="toggleSelect(item)"
                             @dblclick="handleOpen(item)"
                             :class="{'bg-blue-500/10': isSelected(item), 'hover:bg-zinc-800/50': !isSelected(item)}"
                             class="grid grid-cols-12 gap-2 sm:gap-4 items-center px-2 sm:px-4 py-3 sm:py-4 border-b border-zinc-800/50 cursor-pointer transition-colors text-sm">
                            
                            <!-- Checkbox -->
                            <div class="col-span-1 flex justify-center">
                                <div :class="isSelected(item) ? 'bg-blue-600 border-blue-600' : 'bg-zinc-800 border-zinc-600 hover:border-zinc-500'" class="w-5 h-5 rounded-md border-2 flex items-center justify-center transition-colors">
                                    <i v-if="isSelected(item)" data-lucide="check" class="w-3 h-3 text-white"></i>
                                </div>
                            </div>
                            
                            <!-- Name -->
                            <div class="col-span-7 sm:col-span-6 flex items-center gap-2 sm:gap-3 min-w-0">
                                <div v-if="item.is_dir" class="w-8 h-8 sm:w-10 sm:h-10 shrink-0 flex items-center justify-center">
                                    <i data-lucide="folder" class="w-8 h-8 sm:w-10 sm:h-10 text-amber-400 fill-amber-400/20"></i>
                                </div>
                                <div v-else class="w-8 h-8 sm:w-10 sm:h-10 rounded-lg bg-zinc-800 flex items-center justify-center shrink-0">
                                    <i data-lucide="file" class="w-4 h-4 sm:w-5 sm:h-5 text-zinc-500"></i>
                                </div>
                                <span class="truncate font-medium text-zinc-300">{{ item.name }}</span>
                            </div>
                            
                            <!-- Size -->
                            <div class="col-span-2 text-right font-mono text-xs text-zinc-500 hidden sm:block">
                                {{ formatSize(item.size) }}
                            </div>
                            
                            <!-- Actions -->
                            <div class="col-span-4 sm:col-span-3 flex justify-end gap-1">
                                <button @click.stop="downloadItem(item)" class="px-2 sm:px-3 py-1.5 bg-blue-500/10 hover:bg-blue-500/20 text-blue-400 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-colors active:scale-95 border border-blue-500/20">
                                    <i data-lucide="download" class="w-3.5 h-3.5"></i>
                                    <span class="hidden sm:inline">{{ item.is_dir ? 'Zip' : 'Download' }}</span>
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </main>
        
        <!-- Footer -->
        <footer class="bg-zinc-900/50 border-t border-zinc-800 py-4 text-center">
             <p class="text-xs text-zinc-500">Powered by <span class="font-semibold text-zinc-400">HFS</span> • Secure Local File Transfer</p>
        </footer>
    </div>

    <script>
        const { createApp, ref, computed, onMounted } = Vue
        
        createApp({
            setup() {
                const items = ref([])
                const currentPath = ref('/')
                const loading = ref(false)
                const viewMode = ref('grid')
                const selectedItems = ref([])

                const breadcrumbs = computed(() => {
                    const parts = currentPath.value.split('/').filter(p => p)
                    let path = ''
                    return parts.map(name => {
                        path += '/' + name
                        return { name, path }
                    })
                })

                async function fetchItems(path) {
                    loading.value = true
                    try {
                        const res = await fetch(`/api/browse?path=${encodeURIComponent(path)}`)
                        items.value = await res.json()
                        currentPath.value = path
                        selectedItems.value = []
                    } catch (e) {
                        console.error(e)
                    } finally {
                        loading.value = false
                        setTimeout(() => lucide.createIcons(), 50)
                    }
                }

                function navigate(path) {
                    fetchItems(path)
                }

                function handleOpen(item) {
                    if (item.is_dir) {
                        navigate(item.path)
                    } else {
                        downloadItem(item)
                    }
                }

                function downloadItem(item) {
                    if (item.is_dir) {
                        window.location.href = `/zip/folder/${item.path}`
                    } else {
                        window.location.href = `/download/${item.path}`
                    }
                }

                function toggleSelect(item) {
                    const idx = selectedItems.value.indexOf(item.path)
                    if (idx > -1) {
                        selectedItems.value.splice(idx, 1)
                    } else {
                        selectedItems.value.push(item.path)
                    }
                }

                function isSelected(item) {
                    return selectedItems.value.includes(item.path)
                }

                function clearSelection() {
                    selectedItems.value = []
                }

                async function downloadSelection() {
                    if (selectedItems.value.length === 0) return
                    
                    const res = await fetch('/zip/selection', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ files: selectedItems.value })
                    })
                    
                    if (res.ok) {
                        const blob = await res.blob()
                        const url = window.URL.createObjectURL(blob)
                        const a = document.createElement('a')
                        a.href = url
                        a.download = 'download.zip'
                        document.body.appendChild(a)
                        a.click()
                        window.URL.revokeObjectURL(url)
                    }
                }

                function formatSize(bytes) {
                    if (bytes === null || bytes === undefined) return '-'
                    if (bytes === 0) return '0 B'
                    const k = 1024
                    const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
                    const i = Math.floor(Math.log(bytes) / Math.log(k))
                    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
                }

                function getExt(name) {
                    const parts = name.split('.')
                    if (parts.length > 1) return parts.pop().slice(0, 4)
                    return 'FILE'
                }

                onMounted(() => {
                    fetchItems('/')
                    lucide.createIcons()
                })

                return {
                    items, currentPath, loading, viewMode, selectedItems,
                    breadcrumbs, getExt,
                    navigate, handleOpen, downloadItem, toggleSelect, isSelected,
                    clearSelection, downloadSelection, formatSize
                }
            }
        }).mount('#app')
    </script>
</body>
</html>
    "##)
}
