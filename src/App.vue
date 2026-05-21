<template>
  <div class="app">
    <TitleBar @openSettings="showSettings = true" />
    <ProjectPathInput />
    <div class="main-area">
      <HistorySidebar ref="historySidebarRef" @selectGroupChat="onSelectGroupChat" />
      <div class="right-panel">
        <TabBar />
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
    <div v-if="permRequests.length > 0" class="perm-overlay">
      <div class="perm-dialog" v-for="req in permRequests" :key="req.id">
        <div class="perm-header">🔐 权限确认</div>
        <div class="perm-body">
          <p class="perm-tool"><strong>{{ req.tool }}</strong></p>
          <p class="perm-reason">{{ req.reason }}</p>
          <p class="perm-file" v-if="req.file">📄 {{ req.file }}</p>
          <p class="perm-cmd" v-if="req.command">$ {{ req.command }}</p>
        </div>
        <div class="perm-actions">
          <button class="perm-allow" @click="respond(req, 'allow', true)">总是允许</button>
          <button class="perm-once" @click="respond(req, 'allow', false)">允许一次</button>
          <button class="perm-deny" @click="respond(req, 'deny', false)">拒绝</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { db } from './services/db'
import { useGlobalStreaming } from './composables/useGlobalStreaming'
import TitleBar from './components/TitleBar.vue'
import ProjectPathInput from './components/ProjectPathInput.vue'
import HistorySidebar from './components/HistorySidebar.vue'
import TabBar from './components/TabBar.vue'
import SettingsPanel from './components/SettingsPanel.vue'

const showSettings = ref(false)
const currentGroupChatId = ref<number | null>(null)
const historySidebarRef = ref<InstanceType<typeof HistorySidebar> | null>(null)
let agentInvokedUnlisten: UnlistenFn | null = null
const activeStreamListeners = new Map<number, UnlistenFn>()
let permUnlisten: UnlistenFn | null = null

interface PermRequest {
  id: string
  tool: string
  reason: string
  file?: string
  command?: string
}
const permRequests = ref<PermRequest[]>([])

async function respond(req: PermRequest, action: string, always: boolean) {
  await invoke('respond_permission', { id: req.id, tool: req.tool, action, always })
  permRequests.value = permRequests.value.filter(r => r.id !== req.id)
}

const onSelectGroupChat = (chatId: number | null) => {
  currentGroupChatId.value = chatId
}

const onGroupChatCreated = (chatId: number) => {
  currentGroupChatId.value = chatId
  historySidebarRef.value?.loadGroupChats()
}

// Global fallback: listen for agent_invoked and capture stream events + save to DB
// This handles cases where the target agent's tab is not yet mounted (router lazy load)
onMounted(async () => {
  // Listen for permission requests
  permUnlisten = await listen<any>('permission_asked', (event) => {
    permRequests.value.push(event.payload)
  })

  agentInvokedUnlisten = await listen<any>('agent_invoked', async (event) => {
    const { target_agent, session_id } = event.payload
    console.log('[agent_invoked] target:', target_agent, 'session:', session_id)

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

    const unlisten = await listen<any>(`agent_stream_${session_id}`, async (ev) => {
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
          if (fullText || fullThinking) {
            await db.addMessage(session_id, 'assistant', fullText, undefined, fullThinking || undefined)
          }
          // Keep stream visible until polling reloads messages from DB, then clear
          setTimeout(() => clearStreamState(session_id), 3000)
          activeStreamListeners.delete(session_id)
          break
        case 'error':
          updateStreamState(session_id, { text: `Error: ${e.content || ''}`, thinking: fullThinking, done: true, toolCallCount })
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

.perm-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  background: rgba(0, 0, 0, 0.5);
  z-index: 2000;
}

.perm-dialog {
  width: 440px;
  background: var(--bg-secondary);
  border-radius: 12px;
  border: 1px solid var(--border-color);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.4);
  overflow: hidden;
}

.perm-header {
  padding: 14px 20px;
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  border-bottom: 1px solid var(--border-color);
}

.perm-body {
  padding: 16px 20px;
}

.perm-tool {
  font-size: 14px;
  color: var(--accent);
  margin: 0 0 8px;
}

.perm-reason {
  font-size: 13px;
  color: var(--text-primary);
  margin: 0 0 4px;
}

.perm-file, .perm-cmd {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0;
  font-family: monospace;
}

.perm-actions {
  display: flex;
  gap: 8px;
  padding: 12px 20px;
  border-top: 1px solid var(--border-color);
  justify-content: flex-end;
}

.perm-allow, .perm-once, .perm-deny {
  padding: 6px 16px;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
  transition: opacity 0.15s;
}

.perm-allow {
  background: var(--accent);
  color: white;
}

.perm-once {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.perm-deny {
  background: #dc3545;
  color: white;
}

.perm-allow:hover, .perm-once:hover, .perm-deny:hover {
  opacity: 0.85;
}
</style>