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
    Html(r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>HFS Client</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script src="https://unpkg.com/vue@3/dist/vue.global.js"></script>
    <script src="https://unpkg.com/lucide@latest"></script>
</head>
<body class="bg-slate-50 text-slate-900 font-sans h-screen flex flex-col">
    <div id="app" class="h-full flex flex-col">
        <!-- Header -->
        <header class="bg-white border-b border-slate-200 sticky top-0 z-20">
            <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 h-16 flex items-center justify-between">
                <div class="flex items-center gap-4">
                    <div class="bg-blue-600 rounded-lg p-1.5">
                       <i data-lucide="network" class="w-6 h-6 text-white"></i>
                    </div>
                    <h1 class="text-xl font-bold bg-gradient-to-r from-slate-900 to-slate-600 bg-clip-text text-transparent">HFS Client</h1>
                </div>
                
                <div class="flex items-center gap-3">
                     <span class="text-xs font-mono bg-slate-100 px-2 py-1 rounded text-slate-500">Connected</span>
                </div>
            </div>
        </header>

        <!-- Main Area -->
        <main class="flex-1 overflow-hidden flex flex-col max-w-7xl mx-auto w-full px-4 sm:px-6 lg:px-8 py-6">
            
            <!-- Toolbar -->
            <div class="bg-white rounded-t-xl border border-slate-200 p-2 flex flex-wrap items-center gap-2 justify-between">
               <!-- Breadcrumbs -->
               <div class="flex items-center gap-1 text-sm overflow-x-auto px-2 scrollbar-hide">
                  <button @click="navigate('/')" class="p-1.5 hover:bg-slate-100 rounded text-slate-500 hover:text-blue-600 transition-colors">
                      <i data-lucide="home" class="w-4 h-4"></i>
                  </button>
                  <template v-for="(part, index) in breadcrumbs" :key="index">
                      <i data-lucide="chevron-right" class="w-4 h-4 text-slate-300"></i>
                      <button @click="navigate(part.path)" class="px-2 py-1 rounded hover:bg-slate-100 text-slate-700 font-medium hover:text-blue-600 transition-colors whitespace-nowrap">
                          {{ part.name }}
                      </button>
                  </template>
               </div>

               <!-- View Actions -->
               <div class="flex items-center gap-2 ml-auto">
                   <div class="flex bg-slate-100 rounded-lg p-1 border border-slate-200">
                        <button @click="viewMode = 'grid'" :class="{'bg-white shadow text-blue-600': viewMode === 'grid', 'text-slate-500 hover:text-slate-700': viewMode !== 'grid'}" class="p-1.5 rounded transition-all">
                            <i data-lucide="layout-grid" class="w-4 h-4"></i>
                        </button>
                        <button @click="viewMode = 'list'" :class="{'bg-white shadow text-blue-600': viewMode === 'list', 'text-slate-500 hover:text-slate-700': viewMode !== 'list'}" class="p-1.5 rounded transition-all">
                            <i data-lucide="list" class="w-4 h-4"></i>
                        </button>
                   </div>
               </div>
            </div>

            <!-- Toolbar Context Actions (Selection) -->
            <div v-if="selectedItems.length > 0" class="bg-blue-50 border-x border-b border-blue-100 px-4 py-2 flex items-center justify-between animate-in slide-in-from-top-2">
                 <div class="text-sm text-blue-800 font-medium flex items-center gap-2">
                     <i data-lucide="check-square" class="w-4 h-4"></i>
                     {{ selectedItems.length }} selected
                 </div>
                 <button @click="downloadSelection" class="bg-blue-600 hover:bg-blue-700 text-white px-4 py-1.5 rounded-lg text-sm font-medium flex items-center gap-2 transition-colors shadow-sm">
                     <i data-lucide="download" class="w-4 h-4"></i>
                     Download as Zip
                 </button>
            </div>
            <div v-else class="h-[1px] bg-slate-200"></div>

            <!-- Content -->
            <div class="flex-1 bg-white border-x border-b border-slate-200 rounded-b-xl overflow-y-auto p-4" @click.self="clearSelection">
                
                <!-- Loading -->
                <div v-if="loading" class="h-full flex items-center justify-center">
                    <i data-lucide="loader-2" class="w-8 h-8 text-blue-500 animate-spin"></i>
                </div>

                <!-- Empty -->
                <div v-else-if="items.length === 0" class="h-full flex flex-col items-center justify-center text-slate-400">
                    <i data-lucide="folder-open" class="w-16 h-16 mb-4 opacity-50"></i>
                    <p>Folder is empty</p>
                </div>

                <!-- Grid View -->
                <div v-else-if="viewMode === 'grid'" class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-8 gap-4">
                    <div v-for="item in items" :key="item.path" 
                         @click.exact="toggleSelect(item)"
                         @dblclick="handleOpen(item)"
                         :class="{'ring-2 ring-blue-500 bg-blue-50': isSelected(item), 'hover:bg-slate-50': !isSelected(item)}"
                         class="group relative p-4 rounded-xl border border-slate-100 cursor-pointer transition-all flex flex-col items-center text-center select-none">
                        
                        <!-- Checkbox Overlay -->
                        <div class="absolute top-2 left-2 opacity-0 group-hover:opacity-100 transition-opacity" :class="{'opacity-100': isSelected(item)}">
                            <input type="checkbox" :checked="isSelected(item)" class="w-4 h-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500 pointer-events-none">
                        </div>

                        <!-- Icon -->
                        <div class="mb-3 text-slate-400 group-hover:scale-110 transition-transform duration-200">
                            <i v-if="item.is_dir" data-lucide="folder" class="w-16 h-16 fill-amber-100 text-amber-400"></i>
                            <i v-else data-lucide="file" class="w-12 h-12"></i>
                        </div>
                        
                        <div class="text-sm font-medium text-slate-700 truncate w-full" :title="item.name">{{ item.name }}</div>
                        <div class="text-xs text-slate-400 mt-1">{{ formatSize(item.size) }}</div>
                    </div>
                </div>

                <!-- List View -->
                <div v-else class="flex flex-col">
                    <div class="grid grid-cols-12 gap-4 px-4 py-2 text-xs font-semibold text-slate-500 border-b border-slate-100 uppercase tracking-wider">
                        <div class="col-span-1 w-6"></div>
                        <div class="col-span-7">Name</div>
                        <div class="col-span-2 text-right">Size</div>
                        <div class="col-span-2 text-right">Action</div>
                    </div>
                    <div v-for="item in items" :key="item.path"
                         @click.exact="toggleSelect(item)"
                         @dblclick="handleOpen(item)"
                         :class="{'bg-blue-50': isSelected(item), 'hover:bg-slate-50': !isSelected(item)}"
                         class="grid grid-cols-12 gap-4 items-center px-4 py-3 border-b border-slate-50 cursor-pointer transition-colors text-sm text-slate-700">
                        
                        <div class="col-span-1">
                             <input type="checkbox" :checked="isSelected(item)" class="w-4 h-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500 pointer-events-none">
                        </div>
                        <div class="col-span-7 flex items-center gap-3">
                            <i v-if="item.is_dir" data-lucide="folder" class="w-5 h-5 fill-amber-100 text-amber-400"></i>
                            <i v-else data-lucide="file" class="w-5 h-5 text-slate-400"></i>
                            <span class="truncate font-medium">{{ item.name }}</span>
                        </div>
                        <div class="col-span-2 text-right font-mono text-xs text-slate-500">
                            {{ formatSize(item.size) }}
                        </div>
                        <div class="col-span-2 flex justify-end">
                            <button @click.stop="downloadItem(item)" class="p-1.5 hover:bg-blue-100 hover:text-blue-600 rounded text-slate-400 transition-colors" title="Download">
                                <i data-lucide="download" class="w-4 h-4"></i>
                            </button>
                        </div>
                    </div>
                </div>

            </div>
        </main>
        
        <footer class="bg-white border-t border-slate-200 py-4 text-center text-xs text-slate-400">
             Powered by HFS
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
                        selectedItems.value = [] // Clear selection on nav
                    } catch (e) {
                        console.error(e)
                    } finally {
                        loading.value = false
                        setTimeout(() => lucide.createIcons(), 100)
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

                onMounted(() => {
                    fetchItems('/')
                    lucide.createIcons()
                })

                return {
                    items, currentPath, loading, viewMode, selectedItems,
                    breadcrumbs,
                    navigate, handleOpen, downloadItem, toggleSelect, isSelected,
                    clearSelection, downloadSelection, formatSize
                }
            }
        }).mount('#app')
    </script>
</body>
</html>
    "#)
}
