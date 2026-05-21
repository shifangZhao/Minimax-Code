<template>
  <div class="agent-view" :class="agentType">
    <div class="agent-header">
      <span class="agent-name">{{ agentName }}</span>
    </div>
    <div class="messages" ref="messagesEl">
      <div
        v-for="(msg, i) in displayMessages"
        :key="i"
        :class="['message', msg.role]"
      >
        <div class="avatar">{{ msg.role === 'user' ? 'U' : 'A' }}</div>
        <div class="content">
          <div v-if="msg.thinking && msg.role === 'assistant'" class="thinking-block">
            <div class="thinking-toggle" @click="toggleThinking(i)">
              思考过程 {{ isThinkingExpanded(i) ? '▾' : '▸' }}
            </div>
            <div v-if="isThinkingExpanded(i)" class="thinking-text" v-html="formatContent(msg.thinking)"></div>
          </div>
          <div v-if="msg.role === 'user'" class="user-msg">
            <div class="text user-text">{{ msg.content }}</div>
            <div v-if="parsedAttachments(msg)" class="user-attachments">
              <div v-for="(att, j) in parsedAttachments(msg)" :key="j" class="msg-att-wrap">
                <img
                  v-if="getImageSrc(att.path)"
                  :src="getImageSrc(att.path)"
                  class="msg-image"
                  :alt="att.name"
                  :title="att.name"
                  @error="onImgError($event)"
                />
                <div v-else class="msg-img-placeholder">🖼 {{ att.name }}</div>
              </div>
            </div>
          </div>
          <div v-else class="text" v-html="formatContent(msg.content)"></div>
          <div class="time" v-if="msg.created_at">{{ formatTime(msg.created_at) }}</div>
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
      <div v-if="(currentStreaming.text || currentStreaming.thinking) && !currentStreaming.done" class="message assistant">
        <div class="avatar">A</div>
        <div class="content">
          <div v-if="currentStreaming.toolCallCount > 0" class="tool-counter">
            已使用 {{ currentStreaming.toolCallCount }} 个工具
          </div>
          <div class="thinking-text" v-if="pacedThinking">{{ pacedThinking }}</div>
          <pre class="streaming-text" v-if="pacedText">{{ pacedText }}</pre>
        </div>
      </div>
    </div>
    <AskDialog
      v-if="pendingAsk"
      :questions="pendingAsk.questions"
      @submit="handleAskSubmit"
      @cancel="handleAskCancel"
    />
    <div v-if="attachedFiles.length > 0" class="attach-preview">
      <div v-for="(f, idx) in attachedFiles" :key="idx" class="attach-item">
        <span class="preview-file-icon">{{ f.kind === 'image' ? '🖼' : '📄' }}</span>
        <span class="preview-name">{{ f.name }}</span>
        <button class="preview-remove" @click="removeAttachment(idx)" title="移除">✕</button>
      </div>
    </div>
    <div class="input-area" v-if="agentType === 'front' && !pendingAsk">
      <button class="attach-btn" @click="onAttachment" title="添加附件">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>
      </button>
      <input
        type="text"
        v-model="inputText"
        :placeholder="inputPlaceholder"
        @keyup.enter="onSend"
        @paste="onPaste"
      />
      <button class="send-btn" @click="onSend" :disabled="loading" title="发送">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onActivated, onDeactivated, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useAgentConversation } from '../composables/useAgentConversation'
import { useGlobalStreaming } from '../composables/useGlobalStreaming'
import { usePacedText } from '../composables/usePacedText'
import { renderMarkdown } from '../composables/useMarkdown'
import AskDialog from '../components/AskDialog.vue'

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
} = useAgentConversation(props.agentType)

const { globalStreamingStates } = useGlobalStreaming()

const messagesEl = ref<HTMLElement>()
const inputText = ref('')
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
  const state = globalStreamingStates.value.get(streamKey.value)
  if (!state) return { text: '', thinking: '', done: true, toolCallCount: 0 }
  return state
})

// Paced text for smooth typewriter streaming effect
const { displayedText: pacedText } = usePacedText(
  () => currentStreaming.value.text,
  () => currentStreaming.value.done,
)
const { displayedText: pacedThinking } = usePacedText(
  () => currentStreaming.value.thinking,
  () => currentStreaming.value.done,
)

const showLoading = computed(() => {
  const cs = currentStreaming.value
  return loading.value && cs && !cs.done && !cs.text && !cs.thinking
})

const displayMessages = computed(() => {
  return messages.value.map(m => {
    // If content starts with 💭, extract thinking and text
    if (m.content && m.content.startsWith('💭')) {
      const parts = m.content.split('\n\n')
      return {
        ...m,
        thinking: parts[0].replace('💭 ', ''),
        content: parts.slice(1).join('\n\n'),
      }
    }
    return {
      ...m,
      thinking: (m as any).thinking,
    }
  })
})

function formatContent(text: string): string {
  return renderMarkdown(text) || ''
}

interface AttInfo { name: string; path: string; kind: string }

function parsedAttachments(msg: any): AttInfo[] | null {
  if (!msg.attachments) return null
  try {
    const arr = typeof msg.attachments === 'string' ? JSON.parse(msg.attachments) : msg.attachments
    if (!Array.isArray(arr) || arr.length === 0) return null
    return arr.filter((a: AttInfo) => a.kind === 'image')
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

async function onSend() {
  const text = inputText.value.trim()
  if ((!text && attachedFiles.value.length === 0) || loading.value) return
  inputText.value = ''

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

watch([pacedText, pacedThinking], () => {
  scrollToBottom()
})


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
}

.agent-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
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
  gap: 10px;
  max-width: 80%;
}

.message.user {
  align-self: flex-end;
  flex-direction: row-reverse;
}

.message.assistant {
  align-self: flex-start;
}

.avatar {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: var(--bg-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  flex-shrink: 0;
}

.content {
  background: var(--bg-secondary);
  border-radius: 12px;
  padding: 10px 14px;
  max-width: 100%;
}

.message.user .content {
  background: var(--bg-tertiary);
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
}

.thinking-toggle:hover {
  opacity: 0.7;
}

.thinking-text {
  font-size: 12px;
  color: var(--text-secondary);
  margin-top: 4px;
  line-height: 1.5;
  word-break: break-word;
  user-select: text;
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

.input-area {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background-color: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
}

.attach-btn {
  width: 36px;
  height: 36px;
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

.attach-btn:hover {
  color: var(--text-primary);
  background: var(--bg-tertiary);
}

.input-area input {
  flex: 1;
  height: 36px;
  padding: 0 12px;
  background-color: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 14px;
  outline: none;
}

.input-area input:focus {
  border-color: var(--accent);
}

.input-area input::placeholder {
  color: var(--text-secondary);
}

.send-btn {
  width: 36px;
  height: 36px;
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

.send-btn:hover:not(:disabled) {
  color: var(--accent);
  background: var(--bg-tertiary);
}

.send-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
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
</style>