<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { open as openShell } from '@tauri-apps/plugin-shell'
import {
  Folder, File as FileIcon, Play, Square, Network,
  Plus, Search, LayoutGrid, List, HardDrive,
  ChevronRight, Home, Trash2, Settings, RefreshCw,
  MoreVertical, Download, X
} from 'lucide-vue-next'
import './assets/css/main.css'

const isRunning = ref(false)
const port = ref(8080)
const ips = ref<string[]>([])
const sharedItems = ref<string[]>([])
const serverUrl = ref('')
const viewMode = ref<'grid' | 'list'>('grid')
const searchQuery = ref('')
const selectedItems = ref<number[]>([])

onMounted(async () => {
  await fetchIps()
})

async function fetchIps() {
  try {
    const result = await invoke<string[]>('get_local_ips')
    ips.value = result
    if (result.length > 0) {
      serverUrl.value = `http://${result[0]}:${port.value}`
    }
  } catch (e) {
    console.error('Failed to get IPs', e)
  }
}

async function startServer() {
  try {
    await invoke('start_server_cmd', { port: port.value, sharedFolders: sharedItems.value })
    isRunning.value = true
  } catch (e) {
    console.error('Failed to start server', e)
    alert('Failed to start server: ' + e)
  }
}

async function stopServer() {
  try {
    await invoke('stop_server_cmd')
    isRunning.value = false
  } catch (e) {
    console.error('Failed to stop server', e)
  }
}

async function selectFiles() {
  try {
    const selected = await open({ multiple: true, directory: false })
    if (selected) {
      const items = Array.isArray(selected) ? selected : [selected]
      addUniqueItems(items)
    }
  } catch (e) { console.error('File selection failed', e) }
}

async function selectFolder() {
  try {
    const selected = await open({ multiple: true, directory: true })
    if (selected) {
      const items = Array.isArray(selected) ? selected : [selected]
      addUniqueItems(items)
    }
  } catch (e) { console.error('Folder selection failed', e) }
}

function addUniqueItems(items: string[]) {
  items.forEach(item => {
    if (!sharedItems.value.includes(item)) {
      sharedItems.value.push(item)
    }
  })
}

function removeItem(item: string) {
  const index = sharedItems.value.indexOf(item)
  if (index > -1) {
    sharedItems.value.splice(index, 1)
    if (selectedItems.value.includes(index)) {
      selectedItems.value = selectedItems.value.filter(i => i !== index)
    }
    // Restart if running to apply changes (MVP limitation)
    if (isRunning.value) {
      stopServer().then(() => startServer())
    }
  }
}

function getFileName(path: string) {
  return path.split(/[/\\]/).pop() || path
}

function getFileExt(path: string) {
  return path.includes('.') ? path.split('.').pop()?.toUpperCase() : 'FILE'
}

function isImage(path: string) {
  const ext = path.split('.').pop()?.toLowerCase()
  return ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].includes(ext || '')
}

const filteredItems = computed(() => {
  if (!searchQuery.value) return sharedItems.value
  return sharedItems.value.filter(item =>
    item.toLowerCase().includes(searchQuery.value.toLowerCase())
  )
})

async function copyToClipboard(text: string) {
  try {
    await navigator.clipboard.writeText(text)
  } catch (e) {
    console.error('Failed to copy', e)
  }
}

async function openUrl(url: string) {
  try {
    await openShell(url)
  } catch (e) {
    console.error('Failed to open url', e)
  }
}
</script>

<template>
  <div class="flex h-screen bg-zinc-950 text-zinc-200 font-sans overflow-hidden selection:bg-blue-500/30">

    <!-- Sidebar -->
    <aside class="w-64 bg-zinc-900/50 border-r border-zinc-800 flex flex-col shrink-0 backdrop-blur-xl">
      <!-- App Brand -->
      <div class="h-14 flex items-center px-4 border-b border-zinc-800 gap-3">
        <div
          class="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-indigo-600 flex items-center justify-center shadow-lg shadow-blue-500/20">
          <Network class="w-5 h-5 text-white" />
        </div>
        <span class="font-bold text-lg tracking-tight text-white">HFS</span>
        <span class="text-xs font-mono text-zinc-500 ml-auto">v0.1</span>
      </div>

      <!-- Server Control -->
      <div class="p-4 space-y-6">
        <div class="space-y-2">
          <div class="flex items-center justify-between text-xs font-medium text-zinc-500 uppercase tracking-widest">
            <span>Server Status</span>
            <span :class="isRunning ? 'text-emerald-400' : 'text-zinc-600'">
              {{ isRunning ? 'ONLINE' : 'OFFLINE' }}
            </span>
          </div>
          <button @click="isRunning ? stopServer() : startServer()"
            class="w-full h-12 rounded-xl font-bold transition-all duration-300 flex items-center justify-center gap-2 group relative overflow-hidden"
            :class="isRunning
              ? 'bg-zinc-800 text-red-400 hover:bg-red-950/30 border border-red-900/50'
              : 'bg-blue-600 hover:bg-blue-500 text-white shadow-lg shadow-blue-500/10'"
            :disabled="!isRunning && sharedItems.length === 0">
            <component :is="isRunning ? Square : Play" class="w-4 h-4 fill-current" />
            <span class="z-10">{{ isRunning ? 'Stop Server' : 'Start Server' }}</span>
          </button>
        </div>

        <!-- Network Info -->
        <div class="space-y-3">
          <div class="text-xs font-medium text-zinc-500 uppercase tracking-widest flex items-center gap-2">
            <Network class="w-3 h-3" /> Network
          </div>

          <div v-if="isRunning && ips.length > 0" class="space-y-2">
            <div v-for="ip in ips" :key="ip" class="group relative">
              <div
                class="p-3 rounded-lg bg-zinc-900 border border-zinc-800 hover:border-blue-500/50 transition-colors group-hover:shadow-[0_0_15px_-3px_rgba(59,130,246,0.15)] flex justify-between items-center group/card">
                <div>
                  <div class="text-[10px] text-zinc-500 font-mono mb-1">LAN URL</div>
                  <div class="font-mono text-blue-400 text-sm truncate">http://{{ ip }}:{{ port }}</div>
                </div>
                <div class="flex gap-2 opacity-0 group-hover/card:opacity-100 transition-opacity">
                  <button @click="copyToClipboard(`http://${ip}:${port}`)"
                    class="p-1.5 hover:bg-zinc-800 rounded text-zinc-400 hover:text-white" title="Copy">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none"
                      stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <rect width="8" height="4" x="8" y="2" rx="1" ry="1" />
                      <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
                      <path d="M12 11h4" />
                      <path d="M12 16h4" />
                      <path d="M8 11h.01" />
                      <path d="M8 16h.01" />
                    </svg>
                  </button>
                  <button @click="openUrl(`http://${ip}:${port}`)"
                    class="p-1.5 hover:bg-zinc-800 rounded text-zinc-400 hover:text-white" title="Open">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none"
                      stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                      <polyline points="15 3 21 3 21 9" />
                      <line x1="10" y1="14" x2="21" y2="3" />
                    </svg>
                  </button>
                </div>
              </div>
            </div>
          </div>
          <div v-else class="p-4 rounded-lg border border-dashed border-zinc-800 text-center text-zinc-600 text-xs">
            Start server to view connection details
          </div>
        </div>
      </div>

      <!-- Storage Stats / Sidebar Footer -->
      <div class="mt-auto p-4 border-t border-zinc-800">
        <div class="flex items-center gap-3 text-sm text-zinc-400">
          <HardDrive class="w-4 h-4" />
          <span>Local Storage</span>
        </div>
        <div class="mt-2 h-1 bg-zinc-800 rounded-full overflow-hidden">
          <div class="h-full w-3/4 bg-zinc-600 rounded-full"></div>
        </div>
        <div class="mt-2 flex justify-between text-[10px] text-zinc-500">
          <span>System Drive</span>
          <span>Ready</span>
        </div>
      </div>
    </aside>

    <!-- Main Content -->
    <main class="flex-1 flex flex-col min-w-0 bg-background">

      <!-- Top Navigation / Toolbar -->
      <header
        class="h-14 border-b border-zinc-800 flex items-center justify-between px-4 bg-zinc-900/80 backdrop-blur top-0 sticky z-10">
        <!-- Breadcrumbs / Path -->
        <div class="flex items-center gap-2 text-sm text-zinc-400 overflow-hidden">
          <button class="p-1.5 hover:bg-zinc-800 rounded-md transition-colors text-blue-400">
            <Home class="w-4 h-4" />
          </button>
          <ChevronRight class="w-4 h-4 text-zinc-600 shrink-0" />
          <span class="font-medium text-zinc-200">Shared Content</span>
          <span class="px-2 py-0.5 rounded-full bg-zinc-800 text-xs text-zinc-500">{{ sharedItems.length }}</span>
        </div>

        <!-- Search & Actions -->
        <div class="flex items-center gap-2">
          <!-- Search -->
          <div class="relative group hidden sm:block">
            <Search
              class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500 group-focus-within:text-blue-400 transition-colors" />
            <input v-model="searchQuery" type="text" placeholder="Search..."
              class="h-9 w-64 bg-zinc-900 border border-zinc-800 rounded-lg pl-9 pr-4 text-sm text-zinc-300 placeholder:text-zinc-600 focus:outline-none focus:border-blue-500/50 focus:ring-1 focus:ring-blue-500/50 transition-all font-sans">
          </div>
        </div>
      </header>

      <!-- Action Bar -->
      <div class="h-12 border-b border-zinc-800 flex items-center px-4 gap-2 bg-zinc-950/50">
        <div class="flex items-center gap-1">
          <button @click="selectFiles"
            class="h-8 px-3 rounded-lg hover:bg-zinc-800 text-sm font-medium flex items-center gap-2 text-zinc-300 transition-colors">
            <div class="w-4 h-4 rounded bg-blue-500/10 flex items-center justify-center text-blue-400">
              <Plus class="w-3 h-3" />
            </div>
            <span>Add Files</span>
          </button>
          <button @click="selectFolder"
            class="h-8 px-3 rounded-lg hover:bg-zinc-800 text-sm font-medium flex items-center gap-2 text-zinc-300 transition-colors">
            <div class="w-4 h-4 rounded bg-amber-500/10 flex items-center justify-center text-amber-400">
              <Folder class="w-3 h-3" />
            </div>
            <span>Add Folder</span>
          </button>
        </div>

        <div class="w-px h-6 bg-zinc-800 mx-2"></div>

        <div class="ml-auto flex items-center gap-1 bg-zinc-900 p-0.5 rounded-lg border border-zinc-800">
          <button @click="viewMode = 'grid'" class="p-1.5 rounded-md transition-all"
            :class="viewMode === 'grid' ? 'bg-zinc-700 text-white shadow-sm' : 'text-zinc-500 hover:text-zinc-300'">
            <LayoutGrid class="w-4 h-4" />
          </button>
          <button @click="viewMode = 'list'" class="p-1.5 rounded-md transition-all"
            :class="viewMode === 'list' ? 'bg-zinc-700 text-white shadow-sm' : 'text-zinc-500 hover:text-zinc-300'">
            <List class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- File Browser Area -->
      <div class="flex-1 overflow-y-auto p-4" @click.self="selectedItems = []">

        <!-- Empty State -->
        <div v-if="sharedItems.length === 0"
          class="h-full flex flex-col items-center justify-center text-zinc-500 pb-20">
          <div
            class="w-24 h-24 rounded-full bg-zinc-900 border border-zinc-800 flex items-center justify-center mb-6 animate-pulse">
            <HardDrive class="w-10 h-10 opacity-50" />
          </div>
          <h3 class="text-lg font-medium text-zinc-300 mb-2">No Shared Content</h3>
          <p class="text-sm max-w-xs text-center mb-8">Add files or folders to start sharing them on your local network.
          </p>
          <div class="flex gap-4">
            <button @click="selectFiles"
              class="px-5 py-2.5 rounded-xl bg-zinc-900 border border-zinc-800 hover:border-zinc-700 hover:bg-zinc-800 transition-all font-medium text-sm flex items-center gap-2">
              <FileIcon class="w-4 h-4 text-blue-400" /> Choose Files
            </button>
          </div>
        </div>

        <!-- Content Grid/List -->
        <div v-else>
          <!-- Grid View -->
          <div v-if="viewMode === 'grid'"
            class="grid grid-cols-2 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 xl:grid-cols-8 gap-4">
            <div v-for="item in filteredItems" :key="item"
              class="group relative flex flex-col items-center text-center p-4 rounded-xl transition-all duration-200 cursor-pointer border border-transparent"
              :class="selectedItems.includes(sharedItems.indexOf(item)) ? 'bg-blue-500/10 border-blue-500/30' : 'hover:bg-zinc-900 border-transparent hover:border-zinc-800'"
              @click="selectedItems = [sharedItems.indexOf(item)]">
              <!-- Icon -->
              <div class="w-16 h-16 mb-3 transition-transform duration-200 group-hover:scale-105 shadow-sm">
                <div v-if="!item.includes('.')" class="w-full h-full flex items-center justify-center">
                  <Folder class="w-16 h-16 text-amber-400 fill-amber-400/20 drop-shadow-lg" />
                </div>
                <!-- Image Preview Mock -->
                <div v-else-if="isImage(item)"
                  class="w-full h-full rounded-lg bg-zinc-800 border border-zinc-700 overflow-hidden flex items-center justify-center">
                  <span class="font-bold text-xs text-zinc-600">{{ getFileExt(item) }}</span>
                </div>
                <div v-else class="w-full h-full flex items-center justify-center relative">
                  <FileIcon class="w-14 h-14 text-zinc-400" />
                  <span
                    class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-[8px] font-bold text-zinc-950 pt-2">{{
                      getFileExt(item).slice(0, 4) }}</span>
                </div>
              </div>

              <!-- Name -->
              <div class="w-full text-xs text-zinc-300 font-medium truncate px-1 select-none">
                {{ getFileName(item) }}
              </div>

              <!-- Hover Actions -->
              <button @click.stop="removeItem(item)"
                class="absolute -top-1 -right-1 w-6 h-6 bg-red-500 rounded-full text-white flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity shadow-lg hover:bg-red-600 scale-90 group-hover:scale-100"
                title="Stop Sharing">
                <X class="w-3 h-3" />
              </button>
            </div>
          </div>

          <!-- List View -->
          <div v-else class="flex flex-col gap-1">
            <div
              class="grid grid-cols-12 gap-4 px-4 py-2 text-xs font-semibold text-zinc-500 uppercase tracking-wider border-b border-zinc-800/50">
              <div class="col-span-6">Name</div>
              <div class="col-span-4">Path</div>
              <div class="col-span-2 text-right">Actions</div>
            </div>
            <div v-for="item in filteredItems" :key="item"
              class="group grid grid-cols-12 gap-4 items-center px-4 py-3 rounded-lg cursor-pointer transition-colors border border-transparent"
              :class="selectedItems.includes(sharedItems.indexOf(item)) ? 'bg-blue-500/10 border-blue-500/20' : 'hover:bg-zinc-900 border-transparent'"
              @click="selectedItems = [sharedItems.indexOf(item)]">
              <div class="col-span-6 flex items-center gap-3">
                <Folder v-if="!item.includes('.')" class="w-5 h-5 text-amber-400 fill-amber-400/20" />
                <FileIcon v-else class="w-5 h-5 text-zinc-500" />
                <span class="text-sm text-zinc-200 truncate font-medium">{{ getFileName(item) }}</span>
              </div>
              <div class="col-span-4 text-xs text-zinc-500 truncate font-mono">
                {{ item }}
              </div>
              <div class="col-span-2 flex justify-end">
                <button @click.stop="removeItem(item)"
                  class="p-2 text-zinc-500 hover:text-red-400 hover:bg-red-400/10 rounded-md transition-all opacity-0 group-hover:opacity-100">
                  <Trash2 class="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
        </div>

      </div>

      <!-- Footer / Status Bar -->
      <footer
        class="h-8 bg-zinc-950 border-t border-zinc-800 flex items-center px-4 justify-between text-[10px] text-zinc-500 select-none">
        <div class="flex items-center gap-4">
          <div class="flex items-center gap-1.5 hover:text-zinc-300 transition-colors cursor-help">
            <span class="w-2 h-2 rounded-full" :class="isRunning ? 'bg-emerald-500' : 'bg-red-500'"></span>
            {{ isRunning ? 'Active' : 'Stopped' }}
          </div>
          <div class="w-px h-3 bg-zinc-800"></div>
          <div>{{ sharedItems.length }} objects</div>
          <div v-if="selectedItems.length > 0">{{ selectedItems.length }} selected</div>
        </div>
        <div class="flex items-center gap-2">
          <span>Ready</span>
          <div class="w-px h-3 bg-zinc-800"></div>
          <div class="font-mono">UTF-8</div>
        </div>
      </footer>

    </main>
  </div>
</template>

<style>
/* Custom Scrollbar for the dark theme */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: #27272a;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: #3f3f46;
}
</style>
