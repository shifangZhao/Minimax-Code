<template>
  <div class="app">
    <TitleBar @openSettings="showSettings = true" @toggleSidebar="sidebarCollapsed = !sidebarCollapsed" />
    <ProjectPathInput />
    <div class="main-area">
      <HistorySidebar ref="historySidebarRef" :collapsed="sidebarCollapsed" @selectGroupChat="onSelectGroupChat" />
      <div class="right-panel">
        <TabBar v-if="$route.path !== '/ace'" />
        <main class="content">
          <router-view v-slot="{ Component, route }">
            <KeepAlive>
              <component
                :is="Component"
                :key="route.path"
                :groupChatId="currentGroupChatId"
                @groupChatCreated="onGroupChatCreated"
                @renameGroupChat="historySidebarRef?.loadGroupChats()"
              />
            </KeepAlive>
          </router-view>
        </main>
      </div>
    </div>
    <SettingsPanel :visible="showSettings" @close="showSettings = false" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { db } from './services/db'
import { check } from '@tauri-apps/plugin-updater'

import { useGlobalStreaming, activeFrontendSessions, type StreamEventPayload } from './composables/useGlobalStreaming'
import { usePermissions } from './composables/usePermissions'
import type { AgentInvokedPayload } from './composables/useAgentConversation'
import TitleBar from './components/TitleBar.vue'
import ProjectPathInput from './components/ProjectPathInput.vue'
import HistorySidebar from './components/HistorySidebar.vue'
import TabBar from './components/TabBar.vue'
import SettingsPanel from './components/SettingsPanel.vue'

const showSettings = ref(false)
const sidebarCollapsed = ref(false)
const currentGroupChatId = ref<number | null>(null)
const historySidebarRef = ref<InstanceType<typeof HistorySidebar> | null>(null)
let agentInvokedUnlisten: UnlistenFn | null = null
const activeStreamListeners = new Map<number, UnlistenFn>()
let permUnlisten: UnlistenFn | null = null

const { permRequests } = usePermissions()

const onSelectGroupChat = (chatId: number | null) => {
  currentGroupChatId.value = chatId
}

const onGroupChatCreated = (chatId: number) => {
  currentGroupChatId.value = chatId
  historySidebarRef.value?.loadGroupChats(chatId)
}

// Global fallback: listen for agent_invoked and capture stream events + save to DB
// This handles cases where the target agent's tab is not yet mounted (router lazy load)
onMounted(async () => {
  // Disable resize ratio overlay on Windows
  try { await getCurrentWindow().setShadow(false) } catch {}

  // Block browser refresh shortcuts (Ctrl+R, F5) but keep devtools (Ctrl+Shift+I)
  document.addEventListener('keydown', (e) => {
    // Ctrl+Shift+I → open devtools
    if (e.ctrlKey && e.shiftKey && e.key === 'I') {
      e.preventDefault()
      invoke('open_devtools')
      return
    }
    // Block Ctrl+R, F5, Ctrl+F5, Ctrl+Shift+R
    if (
      (e.ctrlKey && e.key === 'r') ||
      e.key === 'F5' ||
      (e.ctrlKey && e.key === 'R')
    ) {
      e.preventDefault()
    }
  })

  // Listen for permission requests
  permUnlisten = await listen<{ id: string; tool: string; file?: string; command?: string; reason: string }>('permission_asked', (event) => {
    permRequests.value.push(event.payload)
  })

  // Auto check for updates on startup
  try {
    const update = await check()
    if (update) {
      console.log('[updater] Update available:', update.version)
      // TODO: show a toast/notification instead of alert
    }
  } catch (e) {
    console.warn('[updater] Check failed:', e)
  }

  agentInvokedUnlisten = await listen<AgentInvokedPayload>('agent_invoked', async (event) => {
    const { target_agent, session_id } = event.payload
    console.log('[agent_invoked] target:', target_agent, 'session:', session_id)

    // If a frontend tab is already handling this session, skip global fallback
    if (activeFrontendSessions.has(session_id)) return

    // Clean up previous listener for same session
    if (activeStreamListeners.has(session_id)) {
      activeStreamListeners.get(session_id)!()
      activeStreamListeners.delete(session_id)
    }

    const { updateStreamState, clearStreamState } = useGlobalStreaming()
    clearStreamState(session_id)

    let fullText = ''
    let fullThinking = ''
    let toolCallCount = 0

    const unlisten = await listen<StreamEventPayload>(`agent_stream_${session_id}`, async (ev) => {
      const e = ev.payload
      switch (e.type) {
        case 'content_block_delta':
          if (e.content) fullText += e.content
          if (e.thinking) fullThinking += e.thinking
          updateStreamState(session_id, { text: fullText, thinking: fullThinking, done: false, toolCallCount })
          break
        case 'tool_start':
          toolCallCount++
          updateStreamState(session_id, { text: fullText, thinking: fullThinking, done: false, toolCallCount })
          break
        case 'done':
          updateStreamState(session_id, { text: fullText, thinking: fullThinking, done: true, toolCallCount })
          // Backend now persists final assistant messages.
          // Keep stream visible briefly then clear.
          setTimeout(() => clearStreamState(session_id), 3000)
          activeStreamListeners.delete(session_id)
          break
        case 'cache_usage':
          console.log(
            `[cache] session=${session_id} hit=${e.cache_hit_tokens} miss=${e.cache_miss_tokens} ratio=${((e.cache_hit_ratio || 0) * 100).toFixed(2)}%`
          )
          break
        case 'error':
          updateStreamState(session_id, { text: `Error: ${e.content || ''}`, thinking: fullThinking, done: true, toolCallCount })
          // Error messages are still saved from frontend (backend may not have persisted)
          if (fullText) {
            await db.addMessage(session_id, 'assistant', fullText + '\n\nError: ' + e.content, undefined, fullThinking || undefined)
          }
          setTimeout(() => clearStreamState(session_id), 3000)
          activeStreamListeners.delete(session_id)
          break
      }
    })

    activeStreamListeners.set(session_id, unlisten)
  })
})

onUnmounted(() => {
  if (agentInvokedUnlisten) {
    agentInvokedUnlisten()
    agentInvokedUnlisten = null
  }
  if (permUnlisten) {
    permUnlisten()
    permUnlisten = null
  }
  for (const [_, unlisten] of activeStreamListeners) {
    unlisten()
  }
  activeStreamListeners.clear()
})
</script>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
  width: 100%;
}

.main-area {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.right-panel {
  display: flex;
  flex-direction: column;
  flex: 1;
  overflow: hidden;
}

.content {
  flex: 1;
  overflow: hidden;
  background-color: var(--bg-primary);
}

</style>