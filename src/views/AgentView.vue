<template>
  <div class="agent-view" :class="agentType">
    <div class="agent-header">
      <span class="agent-name">{{ agentName }}</span>
      <div class="header-actions">
        <button
          v-if="loading"
          class="header-btn header-btn-stop"
          title="停止生成"
          :disabled="stopClicked"
          @click="stopStream()"
        >■</button>
        <button
          v-if="recentEdits.length > 0"
          class="header-btn"
          title="撤销编辑"
          @click="sessionId && undoLast(sessionId)"
        >↩</button>
        <button
          class="header-btn"
          title="快照"
          @click="showBookmarkPanel = !showBookmarkPanel"
        >📸</button>
        <button
          class="header-btn header-btn-danger"
          title="清空对话"
          @click="showClearConfirm = true"
        >🗑</button>
      </div>
    </div>
    <BookmarkPanel
      :visible="showBookmarkPanel"
      :items="bookmarks"
      :showInput="showSaveInput"
      @save="(name: string) => { bookmarkName = name; sessionId && saveBookmark(sessionId, workspace); }"
      @restore="(bm: any) => { showRestoreConfirmBm = bm }"
      @delete="(id: number) => { sessionId && deleteBookmark(id, sessionId) }"
      @toggleInput="showSaveInput = !showSaveInput"
    />
    <div v-if="showRestoreConfirmBm" class="confirm-overlay">
      <div class="confirm-card">
        <p>恢复快照 "{{ showRestoreConfirmBm.name }}"？这将覆盖所有已跟踪的文件。</p>
        <div class="confirm-actions">
          <button class="confirm-cancel" @click="showRestoreConfirmBm = null">取消</button>
          <button class="confirm-danger" @click="handleRestoreBookmark()">恢复</button>
        </div>
      </div>
    </div>
    <div v-if="showClearConfirm" class="confirm-overlay">
      <div class="confirm-card">
        <p>清空此对话的所有消息？</p>
        <div class="confirm-actions">
          <button class="confirm-cancel" @click="showClearConfirm = false">取消</button>
          <button class="confirm-danger" @click="clearConversation()">清空</button>
        </div>
      </div>
    </div>
    <div v-if="messages.length > 0 || loading" class="context-usage">
      <div class="context-bar">
        <div class="context-fill" :class="usageColor" :style="{ width: Math.min(tokenUsage.usage_pct, 100) + '%' }"></div>
      </div>
      <span class="context-label">{{ formatTokens(tokenUsage.estimated_tokens) }} / {{ formatTokens(tokenUsage.context_window) }}</span>
    </div>
    <div class="messages" ref="messagesEl">
      <div
        v-for="(msg, i) in displayMessages"
        :key="i"
        :class="['message', msg.role]"
      >
        <div v-if="msg.role !== 'tool' && msg.role !== 'system'" class="avatar">{{ msg.role === 'user' ? 'U' : 'A' }}</div>
        <div class="content">
          <div v-if="msg.thinking && msg.role === 'assistant'" class="thinking-block">
            <div class="thinking-toggle" :class="{ collapsed: !isThinkingExpanded(i) }" @click="toggleThinking(i)">
              思考过程
              <span class="toggle-arrow"></span>
            </div>
            <div v-if="isThinkingExpanded(i)" class="thinking-text" v-html="formatContent(msg.thinking)"></div>
          </div>
          <div v-if="msg.role === 'user'" class="user-msg">
            <div v-if="parsedAttachments(msg)" class="user-attachments">
              <div v-for="(att, j) in parsedAttachments(msg)" :key="j" class="msg-att-wrap">
                <template v-if="att.kind === 'image'">
                  <img
                    v-if="getImageSrc(att.path)"
                    :src="getImageSrc(att.path)"
                    class="msg-image"
                    :alt="att.name"
                    :title="att.name"
                    @error="onImgError($event)"
                  />
                  <div v-else class="msg-img-placeholder">🖼 {{ att.name }}</div>
                </template>
                <div v-else class="msg-file-badge">📄 {{ att.name }}</div>
              </div>
            </div>
            <div class="text user-text">{{ msg.content }}</div>
          </div>
          <div v-else-if="msg.role === 'tool'" class="tool-msg">
            <ToolCard v-if="getToolCardInfo(msg)" :toolInfo="getToolCardInfo(msg)!" />
          </div>
          <div v-else-if="msg.role === 'system'" class="system-msg">
            <template v-if="msg.loading">
              <span class="cmd-spinner"></span> {{ msg.content }}
            </template>
            <template v-else>{{ msg.content }}</template>
          </div>
          <div v-else class="text" v-html="formatContent(msg.content)"></div>
          <div class="time" v-if="msg.created_at">{{ formatTime(msg.created_at) }}</div>
        </div>
        <div v-if="msg.role === 'user'" class="msg-hover-actions">
          <button class="hover-btn" title="重新生成" @click="retryMessage(i)">⟳</button>
          <button class="hover-btn" title="回退到此" @click="rewindToMessage(msg.id, msg.content)">↩</button>
        </div>
      </div>
      <div v-if="showLoading" class="message assistant">
        <div class="avatar">A</div>
        <div class="content loading-content">
          <div class="loading-dots">
            <span></span><span></span><span></span>
          </div>
        </div>
      </div>
      <div v-if="(currentStreaming.text || currentStreaming.thinking || streamingToolCards.length > 0) && !currentStreaming.done" class="message assistant">
        <div class="avatar">A</div>
        <div class="content">
          <div
            v-for="card in streamingToolCards"
            :key="card.name + (card.args || '')"
            class="tool-msg"
          >
            <ToolCard :toolInfo="card" />
          </div>
          <div v-if="currentStreaming.toolCallCount > 0 && streamingToolCards.length === 0" class="tool-counter">
            已使用 {{ currentStreaming.toolCallCount }} 个工具
          </div>
          <div class="thinking-text" v-if="currentStreaming.thinking">{{ currentStreaming.thinking }}</div>
          <pre class="streaming-text" v-if="currentStreaming.text && !currentStreaming.done">{{ currentStreaming.text }}</pre>
        </div>
      </div>
    </div>
    <div v-if="showRewindConfirm" class="confirm-overlay">
      <div class="confirm-card">
        <p>回退至此消息发送前的状态，该消息及之后的所有内容将被删除。</p>
        <div class="confirm-actions">
          <button class="confirm-cancel" @click="cancelRewind()">取消</button>
          <button class="confirm-danger" @click="handleRewindConfirm()">确认回退</button>
        </div>
      </div>
    </div>
    <ToastBar
      :visible="showUndoToast"
      :message="lastUndone ? `已撤销: ${lastUndone}` : ''"
      @close="showUndoToast = false"
    />
    <AskDialog
      v-if="pendingAsk"
      :questions="pendingAsk.questions"
      @submit="handleAskSubmit"
      @cancel="handleAskCancel"
    />
    <div v-if="permRequests.length > 0" class="perm-panels">
      <div class="perm-card" v-for="req in permRequests" :key="req.id">
        <div class="perm-header">权限确认 — {{ req.tool }}</div>
        <div class="perm-body">
          <p class="perm-reason" v-if="req.reason">{{ req.reason }}</p>
          <p class="perm-file" v-if="req.file">文件: {{ req.file }}</p>
          <p class="perm-cmd" v-if="req.command">$ {{ req.command }}</p>
        </div>
        <div class="perm-actions">
          <button class="perm-allow" @click="respondPerm(req, 'allow', true)">总是允许</button>
          <button class="perm-once" @click="respondPerm(req, 'allow', false)">允许一次</button>
          <button class="perm-deny" @click="respondPerm(req, 'deny', false)">拒绝</button>
        </div>
      </div>
    </div>
    <div v-if="attachedFiles.length > 0" class="attach-preview">
      <div v-for="(f, idx) in attachedFiles" :key="idx" class="attach-item">
        <span class="preview-file-icon">{{ f.kind === 'image' ? '🖼' : '📄' }}</span>
        <span class="preview-name">{{ f.name }}</span>
        <button class="preview-remove" @click="removeAttachment(idx)" title="移除">✕</button>
      </div>
    </div>
    <div v-if="showCommands && (agentType === 'front' || agentType === 'ace')" class="cmd-popup">
      <div class="cmd-header">
        <span class="cmd-title">命令</span>
        <button class="cmd-close" @click="showCommands = false">×</button>
      </div>
      <div
        v-for="cmd in filteredCommands"
        :key="cmd.name"
        class="cmd-item"
        @click="insertCommand(cmd.name)"
      >
        <span class="cmd-name">{{ cmd.name }}</span>
        <span class="cmd-desc">{{ cmd.desc }}</span>
      </div>
      <div v-if="filteredCommands.length === 0" class="cmd-empty">无匹配命令</div>
    </div>
    <div class="input-area" v-if="agentType === 'front' || agentType === 'ace'">
      <textarea
        ref="inputEl"
        v-model="inputText"
        :placeholder="inputPlaceholder"
        @keydown.enter.exact="onSendKey"
        @keydown.ctrl.enter.exact="onNewline"
        @paste="onPaste"
        @input="autoResize"
        rows="1"
      ></textarea>
      <div class="input-toolbar">
        <div class="toolbar-left">
          <button class="toolbar-btn" @click="onAttachment" title="添加附件">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>
          </button>
          <button class="toolbar-btn" @click="showCommands = !showCommands; manualCommands = showCommands" title="命令">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="17" y1="2" x2="7" y2="22"/></svg>
          </button>
        </div>
        <button v-if="!loading" class="send-btn" @click="onSend" title="发送">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/></svg>
        </button>
        <button v-else class="send-btn stop-btn" @click="stopStream()" title="停止生成">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="4" y="4" width="16" height="16" rx="2"/></svg>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onActivated, onDeactivated, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { db } from '../services/db'
import { useAgentConversation } from '../composables/useAgentConversation'
import { useGlobalStreaming } from '../composables/useGlobalStreaming'
import { usePermissions } from '../composables/usePermissions'
import { renderMarkdown } from '../composables/useMarkdown'
import AskDialog from '../components/AskDialog.vue'
import ToastBar from '../components/ToastBar.vue'
import BookmarkPanel from '../components/BookmarkPanel.vue'
import ToolCard from '../components/ToolCard.vue'
import { useUndoHistory } from '../composables/useUndoHistory'
import { useBookmarks } from '../composables/useBookmarks'

const props = defineProps<{
  agentType: string
  groupChatId?: number | null
}>()

const emit = defineEmits<{
  (e: 'groupChatCreated', chatId: number): void
  (e: 'renameGroupChat', name: string): void
}>()

const {
  messages,
  loading,
  initSession,
  sendMessage,
  currentGroupChatId,
  sessionId,
  pendingAsk,
  clearAsk,
  retryMessage,
  showRewindConfirm,
  rewindToMessage,
  confirmRewind,
  cancelRewind,
  showClearConfirm,
  clearConversation,
  tokenUsage,
  loadMessages,
  displayMessages,
} = useAgentConversation(props.agentType)

const { sessions } = useGlobalStreaming()
const { permRequests, respond: respondPerm } = usePermissions()
const { recentEdits, lastUndone, showUndoToast, loadEdits, undoLast } = useUndoHistory()
const { bookmarks, showBookmarkPanel, showSaveInput, bookmarkName, loadBookmarks, saveBookmark, restoreBookmark, deleteBookmark } = useBookmarks()

const messagesEl = ref<HTMLElement>()
const inputEl = ref<HTMLTextAreaElement>()
const inputText = ref('')
const showCommands = ref(false)
const manualCommands = ref(false)  // true when user clicked / button
const stopClicked = ref(false)

const commands = [
  { name: '/compact', desc: '手动压缩上下文，减少 token 占用' },
  { name: '/mcp reload', desc: '重载 MCP 服务器配置' },
]

const cmdQuery = computed(() => {
  const t = inputText.value.trimStart()
  if (t.startsWith('/')) return t.slice(1)
  return ''
})

const filteredCommands = computed(() => {
  if (!cmdQuery.value) return commands
  const q = cmdQuery.value.toLowerCase()
  return commands.filter(c => c.name.toLowerCase().includes(q) || c.desc.toLowerCase().includes(q))
})

// Show popup when typing / as first char, or when button toggled
watch(inputText, (val) => {
  if (manualCommands.value) return  // manual toggle, don't interfere
  const t = val.trimStart()
  if (t.startsWith('/')) {
    showCommands.value = true
  } else {
    showCommands.value = false
  }
})

function insertCommand(name: string) {
  inputText.value = name + ' '
  showCommands.value = false
  manualCommands.value = false
  inputEl.value?.focus()
}
const thinkingExpanded = ref<Record<number, boolean>>({})
// Per-agent scroll position cache (survives KeepAlive tab switches)
const scrollCache = new Map<string, number>()
const agentScrollKey = computed(() => `scroll_${props.agentType}`)

function isThinkingExpanded(idx: number): boolean {
  // Default to expanded; only collapse if explicitly set to false
  return thinkingExpanded.value[idx] !== false
}

function toggleThinking(idx: number) {
  thinkingExpanded.value[idx] = !isThinkingExpanded(idx)
}

// Compute stream key for this agent+session combination
const streamKey = computed(() => `agent_stream_${sessionId.value ?? 'null'}`)

const agentName = computed(() => {
  const names: Record<string, string> = {
    ace: 'Ace',
    front: 'Front',
    plan: 'Plan',
    work: 'Work',
    review: 'Review',
    explore: 'Explore',
  }
  return names[props.agentType] || props.agentType
})

const inputPlaceholder = computed(() => {
  return `与 ${agentName.value} 对话...`
})

const currentStreaming = computed(() => {
  const entry = sessions.value.get(streamKey.value)
  if (!entry?.state) return { text: '', thinking: '', done: true, toolCallCount: 0, toolEvents: [] as any[] }
  return entry.state
})

const streamingToolCards = computed(() => {
  const events = currentStreaming.value.toolEvents || []
  const cards = new Map<string, { name: string; args?: string; result?: string; state: string }>()
  for (const ev of events) {
    const existing = cards.get(ev.tool_id)
    if (ev.type === 'tool_start') {
      cards.set(ev.tool_id, {
        name: ev.tool,
        args: ev.input ? JSON.stringify(ev.input) : undefined,
        result: existing?.result,
        state: 'running',
      })
    } else if (ev.type === 'tool_end') {
      cards.set(ev.tool_id, {
        name: ev.tool,
        args: existing?.args,
        result: ev.result,
        state: 'done',
      })
    }
  }
  return [...cards.values()]
})


const showLoading = computed(() => {
  const cs = currentStreaming.value
  return loading.value && cs && !cs.done && !cs.text && !cs.thinking
})

function formatContent(text: string, streaming?: boolean): string {
  if (streaming && text) {
    // Defer unclosed fenced code blocks — prevent half the page from
    // flashing into code-style while the LLM is still typing.
    const lastOpen = text.lastIndexOf('```')
    if (lastOpen >= 0 && text.slice(lastOpen).split('\n').filter(l => l.trim().startsWith('```')).length % 2 !== 0) {
      text = text.slice(0, lastOpen)
    }
  }
  return renderMarkdown(text) || ''
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(0) + 'K'
  return n.toString()
}

const usageColor = computed(() => {
  if (tokenUsage.value.usage_pct >= 90) return 'danger'
  if (tokenUsage.value.usage_pct >= 80) return 'warning'
  return ''
})

interface AttInfo { name: string; path: string; kind: string }

function parsedAttachments(msg: any): AttInfo[] | null {
  if (!msg.attachments) return null
  try {
    const arr = typeof msg.attachments === 'string' ? JSON.parse(msg.attachments) : msg.attachments
    if (!Array.isArray(arr) || arr.length === 0) return null
    return arr
  } catch { return null }
}

const imageDataUrls = ref<Record<string, string>>({})
const imgLoading = new Set<string>()

function getImageSrc(p: string): string {
  if (imageDataUrls.value[p]) return imageDataUrls.value[p]
  if (!imgLoading.has(p)) {
    imgLoading.add(p)
    invoke<string>('read_file_base64', { path: p }).then(dataUrl => {
      imageDataUrls.value = { ...imageDataUrls.value, [p]: dataUrl }
    }).catch(() => {
      imageDataUrls.value = { ...imageDataUrls.value, [p]: '' }
    })
  }
  return ''
}

function onImgError(e: Event) {
  // Hide broken images
  const img = e.target as HTMLImageElement
  img.style.display = 'none'
}

function formatTime(ts: string): string {
  return new Date(ts).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}

interface ToolCardInfo { name: string; args?: string; result?: string }

function getToolCardInfo(msg: any): ToolCardInfo | null {
  if (!msg.tool_calls) return null
  try {
    const calls = JSON.parse(msg.tool_calls)
    if (!Array.isArray(calls) || calls.length === 0) return null
    const tc = calls[0]
    return {
      name: tc.function?.name || 'tool',
      args: tc.function?.arguments || undefined,
      result: msg.content || undefined,
    }
  } catch { return null }
}

function isAtBottom(): boolean {
  if (!messagesEl.value) return true
  const el = messagesEl.value
  return el.scrollHeight - el.scrollTop - el.clientHeight < 80
}

function saveScrollPos() {
  if (messagesEl.value) {
    scrollCache.set(agentScrollKey.value, messagesEl.value.scrollTop)
  }
}

function restoreScrollPos() {
  // nextTick waits for Vue DOM updates, then double-RAF waits for browser layout + paint.
  // Without this, KeepAlive-reactivated DOM won't have its scrollHeight computed yet
  // and setting scrollTop silently fails.
  nextTick(() => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (messagesEl.value) {
          const pos = scrollCache.get(agentScrollKey.value)
          if (pos !== undefined) {
            messagesEl.value.scrollTop = pos
          } else {
            messagesEl.value.scrollTop = messagesEl.value.scrollHeight
          }
        }
      })
    })
  })
}

function scrollToBottom(force = false) {
  // Use RAF to wait for browser layout after Vue renders new messages
  requestAnimationFrame(() => {
    if (messagesEl.value && (force || isAtBottom())) {
      messagesEl.value.scrollTop = messagesEl.value.scrollHeight
      saveScrollPos()
    }
  })
}

interface AttachedFile {
  name: string
  path: string
  kind: 'image' | 'text'
  content?: string
}
const attachedFiles = ref<AttachedFile[]>([])

const MAX_FILES = 5
const IMG_EXT = new Set(['jpg','jpeg','png','webp'])

function ext(filename: string) {
  return filename.split('.').pop()?.toLowerCase() || ''
}

async function onAttachment() {
  if (attachedFiles.value.length >= MAX_FILES) return
  const selected = await open({
    multiple: true,
    filters: [{
      name: '所有支持的文件',
      extensions: ['jpg','jpeg','png','webp','txt','md','json','rs','ts','tsx','js','jsx','py','go','java','c','cpp','h','hpp','css','html','yaml','yml','toml','xml','sh','bash','log','env','cfg','ini','sql','vue','svelte'],
    }],
  })
  if (!selected) return

  const paths = Array.isArray(selected) ? selected : [selected]
  for (const p of paths) {
    if (attachedFiles.value.length >= MAX_FILES) break
    const name = p.split(/[\\/]/).pop() || p
    if (IMG_EXT.has(ext(name))) {
      attachedFiles.value = [...attachedFiles.value, { name, path: p, kind: 'image' }]
    } else {
      try {
        const content = await invoke<string>('read_file', { path: p })
        if (content.startsWith('Error')) continue
        attachedFiles.value = [...attachedFiles.value, { name, path: p, kind: 'text', content }]
      } catch { continue }
    }
  }
}

async function onPaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items
  if (!items) return

  for (const item of Array.from(items)) {
    if (attachedFiles.value.length >= MAX_FILES) break
    if (!item.type.startsWith('image/')) continue

    const file = item.getAsFile()
    if (!file) continue
    if (file.size > 50 * 1024 * 1024) continue // 50MB

    // Read clipboard image, save to temp, pass path
    const reader = new FileReader()
    reader.onload = async () => {
      const dataUrl = reader.result as string
      const name = `paste_${Date.now()}_${attachedFiles.value.length}.png`
      try {
        const path = await invoke<string>('save_temp_file', { name, dataUrl })
        attachedFiles.value = [...attachedFiles.value, { name, path, kind: 'image' }]
      } catch { /* ignore failed paste */ }
    }
    reader.readAsDataURL(file)
    e.preventDefault()
  }
}

function removeAttachment(idx: number) {
  attachedFiles.value = attachedFiles.value.filter((_, i) => i !== idx)
}

function onSendKey(e: KeyboardEvent) {
  if (!e.shiftKey) {
    e.preventDefault()
    onSend()
  }
}

function onNewline(e: KeyboardEvent) {
  e.preventDefault()
  const el = e.target as HTMLTextAreaElement
  const start = el.selectionStart
  const end = el.selectionEnd
  inputText.value = inputText.value.slice(0, start) + '\n' + inputText.value.slice(end)
  // Move cursor after the inserted newline
  requestAnimationFrame(() => {
    el.selectionStart = el.selectionEnd = start + 1
  })
}

function autoResize(e: Event) {
  const el = e.target as HTMLTextAreaElement
  el.style.height = 'auto'
  el.style.height = Math.min(el.scrollHeight, 200) + 'px'
  el.style.overflowY = el.scrollHeight > 200 ? 'auto' : 'hidden'
}

function stopStream() {
  if (sessionId.value === null) return
  stopClicked.value = true
  const entry = sessions.value.get(streamKey.value)
  if (entry?.state?.abort) {
    entry.state.abort()
  }
}

async function onSend() {
  const text = inputText.value.trim()

  // Slash commands — show as system messages, not sent to API
  if ((text === '/mcp reload' || text === '/compact') && sessionId.value) {
    inputText.value = ''
    const now = new Date().toISOString()
    const cmdId = Date.now()
    // Push command message to UI and DB (system role — excluded from API context)
    const cmdMsg = {
      id: cmdId,
      session_id: sessionId.value,
      role: 'system' as const,
      content: `$ ${text}`,
      created_at: now,
    } as any
    messages.value.push(cmdMsg)
    await db.addMessage(sessionId.value, 'system', `$ ${text}`)

    // Push a loading spinner placeholder
    const loadingId = cmdId + 1
    const loadingMsg = {
      id: loadingId,
      session_id: sessionId.value,
      role: 'system' as const,
      content: '⏳',
      loading: true,
      created_at: now,
    } as any
    messages.value.push(loadingMsg)
    scrollToBottom(true)

    const removeLoading = () => {
      messages.value = messages.value.filter(m => (m as any).id !== loadingId)
    }

    try {
      let resultContent: string
      if (text === '/compact') {
        const result = await db.compactSession(sessionId.value)
        const freed = result.before - result.after
        const freedKb = (freed / 1024).toFixed(1)
        const beforeK = formatTokens(result.before)
        const afterK = formatTokens(result.after)
        const pct = result.before > 0 ? Math.round((freed / result.before) * 100) : 0
        resultContent = `✅ 压缩完成: ${beforeK} → ${afterK} tokens, 释放 ${freedKb}K (${pct}%)`
        removeLoading()
        messages.value.push({
          id: Date.now(),
          session_id: sessionId.value,
          role: 'system' as const,
          content: resultContent,
          created_at: new Date().toISOString(),
        } as any)
        await db.addMessage(sessionId.value, 'system', resultContent)
        await loadMessages()
      } else {
        const mcpResult = await invoke<string>('mcp_reload')
        resultContent = `✅ MCP 重载 — ${mcpResult}`
        removeLoading()
        messages.value.push({
          id: Date.now(),
          session_id: sessionId.value,
          role: 'system' as const,
          content: resultContent,
          created_at: new Date().toISOString(),
        } as any)
        await db.addMessage(sessionId.value, 'system', resultContent)
      }
    } catch (e: any) {
      removeLoading()
      const err = typeof e === 'string' ? e : (e?.message || '未知错误')
      messages.value.push({
        id: Date.now(),
        session_id: sessionId.value,
        role: 'system' as const,
        content: `❌ 命令失败: ${err}`,
        created_at: new Date().toISOString(),
      } as any)
      await db.addMessage(sessionId.value, 'system', `❌ 命令失败: ${err}`)
      console.error(`[${text}] failed:`, e)
    }
    scrollToBottom(true)
    return
  }

  showCommands.value = false
  manualCommands.value = false
  if ((!text && attachedFiles.value.length === 0) || loading.value) return
  inputText.value = ''
  stopClicked.value = false

  if (attachedFiles.value.length > 0) {
    const files = [...attachedFiles.value]
    attachedFiles.value = []

    const images = files.filter(f => f.kind === 'image')
    const texts = files.filter(f => f.kind === 'text')

    // Render message in UI immediately (before slow vision analysis)
    const fileNames = files.map(f => f.name).join(', ')
    const displayContent = text || `[附件] ${fileNames}`
    const att = JSON.stringify(files.map(f => ({ name: f.name, path: f.path, kind: f.kind })))
    const fakeUserId = Date.now()
    const fakeAsstId = fakeUserId + 1
    messages.value.push({
      id: fakeUserId,
      session_id: sessionId.value!,
      role: 'user' as const,
      content: displayContent,
      attachments: att,
      created_at: new Date().toISOString(),
    } as any)
    // Show fake assistant message while analyzing
    messages.value.push({
      id: fakeAsstId,
      session_id: sessionId.value!,
      role: 'assistant' as const,
      content: `🖼 正在分析图片，请稍候...`,
      created_at: new Date().toISOString(),
    } as any)
    scrollToBottom(true)

    // Now analyze images (may take 5-30s)
    const visionResults: string[] = []
    for (const img of images) {
      try {
        const desc = await invoke<string>('understand_image', {
          imageUrl: img.path,
          prompt: text || '请详细描述这张图片的内容',
        })
        visionResults.push(`[视觉预分析: ${img.name}]\n${desc}`)
      } catch (e: any) {
        const err = typeof e === 'string' ? e : (e?.message || '未知错误')
        visionResults.push(`[视觉预分析失败: ${img.name}] ${err}`)
      }
    }

    // Build enriched context for agent
    const contextParts: string[] = [displayContent]
    if (visionResults.length > 0) contextParts.push(visionResults.join('\n\n'))
    if (texts.length > 0) {
      for (const t of texts) {
        contextParts.push(`[文件: ${t.name}]\n\`\`\`\n${t.content}\n\`\`\``)
      }
    }

    // Remove the fake UI placeholders; sendMessage will save + push real messages
    messages.value = messages.value.filter(m => {
      const id = (m as any).id
      return id !== fakeUserId && id !== fakeAsstId
    })
    await sendMessage(contextParts.join('\n\n'), att, displayContent)
    return
  }
  const isNewChat = !currentGroupChatId.value
  await sendMessage(text)
  scrollToBottom(true)

  if (isNewChat && currentGroupChatId.value) {
    emit('groupChatCreated', currentGroupChatId.value)
  }
}

async function handleAskSubmit(answers: { questionId: string; selected: string[]; freeText: string }[]) {
  const askId = pendingAsk.value?.id
  if (askId) {
    await invoke('respond_ask', { id: askId, answers: JSON.stringify(answers) })
  }
  clearAsk()
}

async function handleAskCancel() {
  const askId = pendingAsk.value?.id
  if (askId) {
    await invoke('respond_ask', { id: askId, answers: JSON.stringify({ cancelled: true }) })
  }
  clearAsk()
}

const workspace = ref('')
const showRestoreConfirmBm = ref<any>(null)

async function handleRestoreBookmark() {
  const bm = showRestoreConfirmBm.value
  if (!bm) return
  await restoreBookmark(bm.id, workspace.value)
  showRestoreConfirmBm.value = null
}

async function handleRewindConfirm() {
  const content = await confirmRewind()
  if (content) {
    inputText.value = content
  }
}

// Load edits and bookmarks when session changes
watch(sessionId, async (sid) => {
  if (sid) {
    loadEdits(sid)
    loadBookmarks(sid)
    // Load workspace for bookmark save
    try {
      workspace.value = await invoke<string>('get_workspace')
    } catch { workspace.value = '' }
  }
})

// Load edits when tool ends (file may have been modified)
watch(() => messages.value.length, async () => {
  if (sessionId.value) {
    await loadEdits(sessionId.value)
  }
})

watch(() => props.groupChatId, async (newId) => {
  if (newId) {
    await initSession(newId)
    scrollToBottom(true)
  } else if (newId === null || newId === undefined) {
    // Group chat was deleted — clear all state
    messages.value = []
    sessionId.value = null
    currentGroupChatId.value = null
  }
}, { immediate: true })

watch(
  () => [currentStreaming.value.text, currentStreaming.value.thinking],
  () => { scrollToBottom() }
)


watch(currentGroupChatId, (newId) => {
  if (newId) {
    emit('groupChatCreated', newId)
  }
})

// Scroll to bottom when a new user message is added
watch(() => messages.value.length, (len, oldLen) => {
  if (len > (oldLen || 0)) {
    const last = messages.value[len - 1]
    if (last.role === 'user') {
      scrollToBottom(true)  // force: user just sent something
    }
  }
})

// Persist scroll position across tab switches (KeepAlive)
// Auto-resize textarea when inputText is cleared programmatically (after send)
watch(inputText, () => {
  nextTick(() => {
    if (inputEl.value) {
      inputEl.value.style.height = 'auto'
      inputEl.value.style.height = Math.min(inputEl.value.scrollHeight, 200) + 'px'
      inputEl.value.style.overflowY = inputEl.value.scrollHeight > 200 ? 'auto' : 'hidden'
    }
  })
})

onMounted(() => {
  messagesEl.value?.addEventListener('scroll', saveScrollPos, { passive: true })
})

onActivated(() => {
  restoreScrollPos()
  messagesEl.value?.addEventListener('scroll', saveScrollPos, { passive: true })
})

onDeactivated(() => {
  saveScrollPos()
  messagesEl.value?.removeEventListener('scroll', saveScrollPos)
})
</script>

<style scoped>
.agent-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.agent-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  position: relative;
}

.agent-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.context-usage {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 12px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
}

.context-bar {
  flex: 1;
  height: 4px;
  background: var(--bg-tertiary);
  border-radius: 2px;
  overflow: hidden;
}

.context-fill {
  height: 100%;
  border-radius: 2px;
  background: var(--accent);
  transition: width 0.3s;
}

.context-fill.warning {
  background: #f0a020;
}

.context-fill.danger {
  background: #e81123;
}

.context-label {
  font-size: 10px;
  color: var(--text-secondary);
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.messages {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.message {
  display: flex;
  gap: 12px;
  align-items: flex-start;
  width: 100%;
  position: relative;
}

.message.user {
  /* left-aligned like DeepSeek */
}

.message.assistant {
  align-self: flex-start;
}

.message.tool {
  align-self: flex-start;
  width: 100%;
}

.message.system {
  align-self: center;
  max-width: 480px;
  padding: 0 16px;
}

.system-msg {
  text-align: center;
  color: var(--text-secondary);
  font-size: 0.82em;
  line-height: 1.5;
  padding: 6px 14px;
  background: var(--bg-tertiary);
  border-radius: 12px;
  white-space: pre-wrap;
  word-break: break-word;
}

.tool-msg {
  flex: 1;
  min-width: 0;
}

/* Role glyph (before avatar) */
.glyph {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--bg-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  font-family: var(--font-mono);
  flex-shrink: 0;
  color: var(--text-secondary);
}

.message.user .glyph {
  color: var(--accent);
  background: var(--bg-tertiary);
}

.message.assistant .glyph {
  color: var(--accent-ok);
  background: var(--bg-tertiary);
}

.message.tool .glyph {
  color: var(--accent-warn);
  background: var(--bg-tertiary);
}

.avatar {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--bg-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  font-family: var(--font-mono);
  flex-shrink: 0;
  color: var(--text-secondary);
}

.message.user .avatar {
  color: var(--accent);
}

.message.assistant .avatar {
  color: var(--accent-ok);
}

.content {
  background: var(--bg-secondary);
  border-radius: 12px;
  padding: 10px 14px;
  flex: 1;
  min-width: 0;
  border: 2px solid transparent;
  position: relative;
}

.message.user .content {
  background: var(--bg-tertiary);
  border-color: #888;
}

.message.assistant .content {
  border-color: var(--accent);
}

.message.tool .content {
  border: none;
  padding: 0;
  background: transparent;
  margin-left: 36px;
}

.loading-content {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 40px;
}

.text {
  color: var(--text-primary);
  font-size: 14px;
  line-height: 1.6;
  word-break: break-word;
  user-select: text;
}

.streaming-text {
  color: var(--text-primary);
  font-size: 14px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  margin: 0;
  font-family: inherit;
  user-select: text;
}

.tool-counter {
  font-size: 12px;
  color: var(--accent-warn);
  margin-bottom: 6px;
  padding: 4px 8px;
  background: var(--bg-tertiary);
  border-radius: 4px;
  display: inline-block;
}

/* Markdown rendered content */
.text :deep(h1) {
  font-size: 20px;
  font-weight: 700;
  margin: 12px 0 8px;
  color: var(--text-primary);
}

.text :deep(h2) {
  font-size: 18px;
  font-weight: 600;
  margin: 10px 0 6px;
  color: var(--text-primary);
}

.text :deep(h3) {
  font-size: 16px;
  font-weight: 600;
  margin: 8px 0 4px;
  color: var(--text-primary);
}

.text :deep(p) {
  margin: 8px 0;
}

.text :deep(code) {
  background: var(--bg-tertiary);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 13px;
  font-family: 'Consolas', 'Monaco', monospace;
  color: var(--accent);
}

.text :deep(pre) {
  background: var(--bg-tertiary);
  border-radius: 8px;
  padding: 12px;
  margin: 8px 0;
  overflow-x: auto;
}

.text :deep(pre code) {
  background: transparent;
  padding: 0;
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-primary);
}

/* Muted highlight.js — low saturation, blends with dark bg */
.text :deep(.hljs)            { color: #b0b0b0; }
.text :deep(.hljs-keyword)    { color: #9b8ec4; }
.text :deep(.hljs-string)     { color: #c4a882; }
.text :deep(.hljs-comment)    { color: #6b6b6b; font-style: italic; }
.text :deep(.hljs-number)     { color: #c4a882; }
.text :deep(.hljs-literal)    { color: #c4a882; }
.text :deep(.hljs-built_in)   { color: #9b8ec4; }
.text :deep(.hljs-type)       { color: #9b8ec4; }
.text :deep(.hljs-function)   { color: #82a8c4; }
.text :deep(.hljs-title)      { color: #82a8c4; }
.text :deep(.hljs-attr)       { color: #c4a882; }
.text :deep(.hljs-params)     { color: #b0b0b0; }
.text :deep(.hljs-meta)       { color: #6b6b6b; }
.text :deep(.hljs-selector-*) { color: #9b8ec4; }
.text :deep(.hljs-tag)        { color: #9b8ec4; }
.text :deep(.hljs-name)       { color: #82a8c4; }

.text :deep(a) {
  color: var(--accent);
  text-decoration: none;
}

.text :deep(a:hover) {
  text-decoration: underline;
}

.text :deep(blockquote) {
  border-left: 3px solid var(--accent);
  margin: 8px 0;
  padding: 4px 12px;
  background: var(--bg-tertiary);
  border-radius: 0 4px 4px 0;
}

.text :deep(ul), .text :deep(ol) {
  margin: 8px 0;
  padding-left: 24px;
}

.text :deep(li) {
  margin: 4px 0;
}

.text :deep(table) {
  border-collapse: collapse;
  margin: 8px 0;
  width: 100%;
}

.text :deep(th), .text :deep(td) {
  border: 1px solid var(--border-color);
  padding: 6px 10px;
  text-align: left;
}

.text :deep(th) {
  background: var(--bg-tertiary);
  font-weight: 600;
}

.text :deep(hr) {
  border: none;
  border-top: 1px solid var(--border-color);
  margin: 16px 0;
}

.user-text {
  color: var(--text-primary);
  white-space: pre-line;
  word-break: break-word;
  user-select: text;
}

.user-attachments {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}

.msg-image {
  max-width: 200px;
  max-height: 150px;
  object-fit: cover;
  border-radius: 6px;
  border: 1px solid var(--border-color);
  cursor: pointer;
}

.msg-image:hover {
  opacity: 0.9;
}

.msg-img-placeholder {
  font-size: 12px;
  color: var(--text-secondary);
  padding: 8px 12px;
  background: var(--bg-tertiary);
  border-radius: 6px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}

.msg-file-badge {
  font-size: 12px;
  color: var(--text-secondary);
  padding: 6px 10px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}

.time {
  font-size: 11px;
  color: var(--text-secondary);
  margin-top: 4px;
}


.tool-counter {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 6px;
  padding: 4px 8px;
  background: var(--bg-tertiary);
  border-radius: 4px;
  display: inline-block;
}

.thinking-block {
  margin-bottom: 6px;
}

.thinking-toggle {
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.toggle-arrow {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-right: 1.5px solid var(--text-secondary);
  border-bottom: 1.5px solid var(--text-secondary);
  transform: rotate(-45deg);
  transition: transform 0.2s;
}

.thinking-toggle.collapsed .toggle-arrow {
  transform: rotate(45deg);
}

.thinking-toggle:hover {
  color: var(--text-primary);
}

.thinking-toggle:hover .toggle-arrow {
  border-color: var(--text-primary);
}

.thinking-text {
  font-size: 12px;
  color: var(--text-secondary);
  margin-top: 4px;
  line-height: 1.5;
  word-break: break-word;
  user-select: text;
}

.thinking-text :deep(p) {
  margin: 6px 0;
}

.thinking-text :deep(ul),
.thinking-text :deep(ol) {
  margin: 6px 0;
  padding-left: 20px;
}

.thinking-text :deep(li) {
  margin: 2px 0;
}

.thinking-text :deep(pre) {
  background: var(--bg-primary);
  border-radius: 4px;
  padding: 8px 12px;
  margin: 6px 0;
  overflow-x: auto;
  font-size: 11px;
}

.thinking-text :deep(code) {
  font-family: 'Consolas', 'Courier New', monospace;
  font-size: 11px;
  background: var(--bg-primary);
  padding: 1px 4px;
  border-radius: 3px;
}

.thinking-text :deep(pre code) {
  background: none;
  padding: 0;
}

.tool-events {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 12px;
  background: var(--bg-tertiary);
  border-radius: 8px;
  margin: 8px 0;
}

.tool-event {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.tool-start {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-primary);
}

.tool-icon {
  font-size: 14px;
}

.tool-name {
  font-weight: 500;
}

.tool-end {
  font-size: 12px;
  color: var(--text-secondary);
  padding-left: 28px;
}

.tool-result {
  background: var(--bg-secondary);
  padding: 4px 8px;
  border-radius: 4px;
  word-break: break-word;
}

.loading-dots {
  display: flex;
  gap: 4px;
  padding: 4px 0;
}

.loading-dots span {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-secondary);
  animation: loading-bounce 1.4s infinite ease-in-out;
}

.loading-dots span:nth-child(1) { animation-delay: 0s; }
.loading-dots span:nth-child(2) { animation-delay: 0.2s; }
.loading-dots span:nth-child(3) { animation-delay: 0.4s; }

@keyframes loading-bounce {
  0%, 80%, 100% { transform: scale(0.6); opacity: 0.5; }
  40% { transform: scale(1); opacity: 1; }
}

.thinking {
  display: flex;
  gap: 4px;
  padding: 4px 0;
}

@keyframes bounce {
  0%, 80%, 100% { transform: scale(0.6); opacity: 0.5; }
  40% { transform: scale(1); opacity: 1; }
}

.attach-preview {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 6px 12px;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
}

.attach-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  background: var(--bg-tertiary);
  border-radius: 8px;
}

.preview-thumb {
  width: 32px;
  height: 32px;
  object-fit: cover;
  border-radius: 4px;
  border: 1px solid var(--border-color);
}

.preview-file-icon {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  flex-shrink: 0;
}

.preview-name {
  max-width: 120px;
  font-size: 11px;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preview-remove {
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.preview-remove:hover {
  background: var(--bg-primary);
  color: var(--text-primary);
}

.cmd-popup {
  margin: 0 12px 4px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  overflow: hidden;
  box-shadow: 0 -4px 12px rgba(0,0,0,0.1);
}

.cmd-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border-color);
}

.cmd-title {
  font-size: 11px;
  color: var(--text-secondary);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.cmd-close {
  width: 18px;
  height: 18px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 14px;
  cursor: pointer;
  border-radius: 3px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.cmd-close:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.cmd-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  cursor: pointer;
  transition: background 0.1s;
}

.cmd-item:hover {
  background: var(--bg-tertiary);
}

.cmd-item + .cmd-item {
  border-top: 1px solid var(--border-color);
}

.cmd-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--accent);
  font-family: 'Consolas', monospace;
  white-space: nowrap;
}

.cmd-desc {
  font-size: 12px;
  color: var(--text-secondary);
}

.cmd-empty {
  padding: 10px 12px;
  font-size: 12px;
  color: var(--text-secondary);
  text-align: center;
}

.input-area {
  display: flex;
  flex-direction: column;
  padding: 10px 12px;
  background-color: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
}

.input-area textarea {
  width: 100%;
  min-height: 40px;
  max-height: 200px;
  padding: 10px 12px;
  background-color: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 14px;
  font-family: inherit;
  line-height: 1.5;
  outline: none;
  resize: none;
  box-sizing: border-box;
  overflow-y: hidden;
  transition: height 0.1s;
}

.input-area textarea:focus {
  border-color: var(--accent);
}

.input-area textarea::placeholder {
  color: var(--text-secondary);
}

.input-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 4px 0;
}

.toolbar-left {
  display: flex;
  gap: 4px;
}

.toolbar-btn {
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.15s, background 0.15s;
}

.toolbar-btn:hover {
  color: var(--text-primary);
  background: var(--bg-tertiary);
}

.send-btn {
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 6px;
  background: var(--accent);
  color: white;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: opacity 0.15s;
}

.send-btn:hover:not(:disabled) {
  opacity: 0.85;
}

.send-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.stop-btn {
  background: #e81123 !important;
}

.stop-btn:hover {
  opacity: 0.85;
}

.rename-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.rename-dialog {
  background: var(--bg-secondary);
  border-radius: 12px;
  padding: 20px 24px;
  width: 360px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
}

.rename-title {
  font-size: 16px;
  font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 16px;
}

.rename-dialog input {
  width: 100%;
  height: 40px;
  padding: 0 12px;
  background: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 14px;
  outline: none;
  margin-bottom: 16px;
}

.rename-dialog input:focus {
  border-color: var(--accent);
}

.rename-btns {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.cancel-btn, .confirm-btn {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  font-size: 14px;
  cursor: pointer;
}

.cancel-btn {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.confirm-btn {
  background: var(--btn-run);
  color: white;
}

/* Permission panels — inline layers above input */
.perm-panels {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0 12px;
}

.perm-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  overflow: hidden;
}

.perm-header {
  padding: 6px 10px;
  font-size: 12px;
  font-weight: 500;
  color: var(--accent);
  border-bottom: 1px solid var(--border-color);
}

.perm-body {
  padding: 6px 10px;
}

.perm-reason {
  font-size: 12px;
  color: var(--text-primary);
  margin: 0 0 4px;
}

.perm-file, .perm-cmd {
  font-size: 11px;
  color: var(--text-secondary);
  margin: 0;
  font-family: monospace;
}

.perm-actions {
  display: flex;
  gap: 4px;
  padding: 5px 10px;
  border-top: 1px solid var(--border-color);
  justify-content: flex-end;
}

.perm-allow, .perm-once, .perm-deny {
  padding: 3px 10px;
  border: none;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
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

/* Header toolbar */
.header-actions {
  display: flex;
  gap: 6px;
  align-items: center;
}

.header-btn {
  padding: 3px 10px;
  border: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border-radius: 4px;
  font-size: 11px;
  cursor: pointer;
  white-space: nowrap;
}

.header-btn:hover {
  background: var(--bg-input);
  color: var(--text-primary);
}

.header-btn-danger:hover {
  color: #e81123;
}

.header-btn-stop {
  color: #e81123;
  border-color: #e81123;
  font-weight: 600;
}

.header-btn-stop:hover:not(:disabled) {
  background: #e81123;
  color: white;
}

/* Message hover actions */
.user-msg {
  position: relative;
}

.msg-hover-actions {
  display: flex;
  flex-direction: column;
  gap: 2px;
  position: absolute;
  top: 28px;
  left: 0;
  opacity: 0;
  transition: opacity 0.15s;
}

.message:hover .msg-hover-actions {
  opacity: 1;
}

.hover-btn {
  width: 22px;
  height: 22px;
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-secondary);
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.hover-btn:hover {
  background: var(--bg-input);
  color: var(--text-primary);
}

/* Confirmation overlay */
.confirm-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
  background: rgba(0,0,0,0.5);
}

.confirm-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 20px 24px;
  min-width: 300px;
  max-width: 400px;
}

.confirm-card p {
  font-size: 14px;
  color: var(--text-primary);
  margin: 0 0 16px 0;
}

.confirm-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.confirm-cancel {
  padding: 5px 14px;
  border: none;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}

.confirm-cancel:hover {
  background: var(--bg-input);
  color: var(--text-primary);
}

.confirm-danger {
  padding: 5px 14px;
  border: none;
  background: #dc3545;
  color: white;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}

.confirm-danger:hover {
  background: #c82333;
}

/* Spinner for slash command loading state */
@keyframes cmd-spin {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

.cmd-spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid var(--border-color);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: cmd-spin 0.7s linear infinite;
  vertical-align: middle;
  margin-right: 4px;
}
</style>