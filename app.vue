<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { FolderOpen, Play, Square, Network, File, Files, Share2, Trash2 } from 'lucide-vue-next'
import './assets/css/main.css'

const isRunning = ref(false)
const port = ref(8080)
const ips = ref<string[]>([])
const sharedItems = ref<string[]>([])
const serverUrl = ref('')

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
    const selected = await open({
      multiple: true,
      directory: false, // Allow files
    })

    if (selected) {
      // open returns string | string[] | null
      const items = Array.isArray(selected) ? selected : [selected]
      // Append unique items
      items.forEach(item => {
        if (!sharedItems.value.includes(item)) {
          sharedItems.value.push(item)
        }
      })
    }
  } catch (e) {
    console.error('File selection failed', e)
  }
}

async function selectFolder() {
  try {
    const selected = await open({
      multiple: true,
      directory: true,
    })

    if (selected) {
      const items = Array.isArray(selected) ? selected : [selected]
      items.forEach(item => {
        if (!sharedItems.value.includes(item)) {
          sharedItems.value.push(item)
        }
      })
    }
  } catch (e) {
    console.error('Folder selection failed', e)
  }
}

function removeItem(index: number) {
  sharedItems.value.splice(index, 1)
  // If running, we should ostensibly restart logic or notify backend, 
  // but for MVP we require restart to update sharing list effectively 
  // unless we implemented hot-reloading of state in backend.
  // Our backend uses Mutex<Vec<String>>, so we COULD update it, 
  // but we didn't expose an update command.
  // Ideally, stopping and starting is fine.
  if (isRunning.value) {
    stopServer().then(() => startServer())
  }
}
</script>

<template>
  <div class="min-h-screen bg-zinc-950 text-white p-8 font-sans selection:bg-blue-500/30">
    <div class="max-w-2xl mx-auto space-y-12">

      <!-- Header -->
      <div class="text-center space-y-2">
        <h1
          class="text-4xl font-bold tracking-tight bg-gradient-to-r from-blue-400 to-indigo-400 bg-clip-text text-transparent">
          HFS
        </h1>
        <p class="text-zinc-400">Minimal HTTP File Share</p>
      </div>

      <!-- Main Controls -->
      <div class="bg-zinc-900/50 rounded-2xl p-6 border border-white/5 backdrop-blur-xl shadow-2xl">
        <div class="flex items-center justify-between gap-6">
          <div class="space-y-1">
            <div class="text-sm font-medium text-zinc-400 uppercase tracking-widest">Status</div>
            <div class="text-2xl font-mono flex items-center gap-3">
              <div class="w-3 h-3 rounded-full animate-pulse"
                :class="isRunning ? 'bg-emerald-500 shadow-[0_0_12px_rgba(16,185,129,0.5)]' : 'bg-red-500 shadow-[0_0_12px_rgba(239,68,68,0.5)]'">
              </div>
              {{ isRunning ? 'ONLINE' : 'OFFLINE' }}
            </div>
          </div>

          <button @click="isRunning ? stopServer() : startServer()"
            class="group relative inline-flex items-center justify-center px-8 py-4 font-bold text-white transition-all duration-200 bg-gradient-to-br rounded-xl focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-zinc-900"
            :class="isRunning
              ? 'from-red-500 to-red-600 hover:from-red-400 hover:to-red-500 ring-red-500/50 shadow-lg shadow-red-500/20'
              : 'from-blue-500 to-indigo-600 hover:from-blue-400 hover:to-indigo-500 ring-blue-500/50 shadow-lg shadow-blue-500/20'"
            :disabled="sharedItems.length === 0">
            <component :is="isRunning ? Square : Play" class="w-5 h-5 mr-2 fill-current" />
            {{ isRunning ? 'STOP SERVER' : 'START SERVER' }}
          </button>
        </div>

        <!-- Connection Info -->
        <div v-if="isRunning"
          class="mt-8 p-4 bg-zinc-950/50 rounded-xl border border-white/5 animate-in fade-in slide-in-from-top-4 duration-300">
          <div class="flex items-center gap-3 mb-2 text-zinc-400">
            <Network class="w-4 h-4" />
            <span class="text-sm font-medium">Available on your network:</span>
          </div>
          <div class="space-y-2">
            <div v-for="ip in ips" :key="ip" class="font-mono text-xl text-blue-400 select-all cursor-text">
              http://{{ ip }}:{{ port }}
            </div>
          </div>
        </div>
      </div>

      <!-- File Selection -->
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-zinc-300 flex items-center gap-2">
            <Share2 class="w-5 h-5" />
            Shared Content
          </h2>
          <div class="flex gap-2">
            <button @click="selectFiles"
              class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-sm font-medium transition-colors flex items-center gap-2 border border-white/5">
              <File class="w-4 h-4" />
              Add Files
            </button>
            <button @click="selectFolder"
              class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-sm font-medium transition-colors flex items-center gap-2 border border-white/5">
              <FolderOpen class="w-4 h-4" />
              Add Folder
            </button>
          </div>
        </div>

        <div class="space-y-2 min-h-[200px]">
          <div v-if="sharedItems.length === 0"
            class="h-[200px] border-2 border-dashed border-zinc-800 rounded-2xl flex flex-col items-center justify-center text-zinc-500 gap-4">
            <Files class="w-12 h-12 opacity-50" />
            <p>No files shared yet. Add some to get started.</p>
          </div>

          <TransitionGroup name="list" tag="div" class="space-y-2">
            <div v-for="(item, index) in sharedItems" :key="item"
              class="group flex items-center justify-between p-4 bg-zinc-900/40 border border-white/5 rounded-xl hover:bg-zinc-900/60 transition-all">
              <div class="flex items-center gap-3 overflow-hidden">
                <div class="w-10 h-10 rounded-lg bg-zinc-800 flex items-center justify-center shrink-0">
                  <FolderOpen v-if="!item.includes('.')" class="w-5 h-5 text-zinc-400" />
                  <File v-else class="w-5 h-5 text-zinc-400" />
                </div>
                <div class="truncate text-sm font-medium text-zinc-300" :title="item">
                  {{ item }}
                </div>
              </div>
              <button @click="removeItem(index)"
                class="p-2 text-zinc-500 hover:text-red-400 hover:bg-red-400/10 rounded-lg transition-colors opacity-0 group-hover:opacity-100">
                <Trash2 class="w-4 h-4" />
              </button>
            </div>
          </TransitionGroup>
        </div>
      </div>

    </div>
  </div>
</template>

<style>
.list-enter-active,
.list-leave-active {
  transition: all 0.3s ease;
}

.list-enter-from,
.list-leave-to {
  opacity: 0;
  transform: translateX(-20px);
}
</style>
