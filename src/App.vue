<template>
  <div class="app">
    <TitleBar @openSettings="showSettings = true" />
    <ProjectPathInput />
    <div class="main-area">
      <HistorySidebar ref="historySidebarRef" @selectGroupChat="onSelectGroupChat" />
      <div class="right-panel">
        <ModeSwitcher />
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
import { db } from './services/db'
import { useGlobalStreaming } from './composables/useGlobalStreaming'
import { usePermissions } from './composables/usePermissions'
import TitleBar from './components/TitleBar.vue'
import ProjectPathInput from './components/ProjectPathInput.vue'
import HistorySidebar from './components/HistorySidebar.vue'
import TabBar from './components/TabBar.vue'
import ModeSwitcher from './components/ModeSwitcher.vue'
import SettingsPanel from './components/SettingsPanel.vue'

const showSettings = ref(false)
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
          // Backend now persists final assistant messages.
          // Keep stream visible briefly then clear.
          setTimeout(() => clearStreamState(session_id), 3000)
          activeStreamListeners.delete(session_id)
          break
        case 'cache_usage':
          console.log(
            `[cache] session=${session_id} hit=${e.cache_hit_tokens} miss=${e.cache_miss_tokens} ratio=${((e.cache_hit_ratio || 0) * 100).toFixed(1)}%`
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