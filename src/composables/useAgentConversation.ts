// Agent conversation using Rust backend via Tauri invoke
// Supports Interleaved Thinking with complete message history

import { ref, computed, onMounted, onUnmounted } from 'vue'
import { db, type ChatMessage, type UIMessage } from '../services/db'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useGlobalStreaming } from './useGlobalStreaming'

export interface ToolEvent {
  type: 'tool_start' | 'tool_end'
  tool: string
  tool_id: string
  input?: Record<string, unknown>
  result?: string
}

export interface AskPayload {
  id: string
  session_id: number
  agent_type?: string
  question?: string
  questions?: Array<{ id: string; question: string; options?: string[]; multi_select?: boolean }>
  options?: string[]
}

export interface AgentInvokedPayload {
  target_agent: string
  session_id: number
  group_chat_id?: number
}

// 完整的 assistant 响应结构（用于多轮对话）
export interface AssistantMessage {
  role: 'assistant'
  content: string
  reasoning_details?: string  // 思考内容
  tool_calls?: Array<{
    id: string
    type: 'function'
    function: {
      name: string
      arguments: string  // JSON string
    }
  }>
}

export interface TokenUsage {
  estimated_tokens: number
  context_window: number
  usage_pct: number
}

// Module-level caches: persist across composable instances so session state
// survives tab switches. This enables multi-session parallel execution —
// each session's messages and token counts live independently.
const sessionTokenUsage = new Map<number, TokenUsage>()
const sessionMessages = new Map<number, ChatMessage[]>()
const sessionMeta = new Map<number, { count: number }>()

export function useAgentConversation(agentType: string) {
  const messages = ref<ChatMessage[]>([])
  const loading = ref(false)  // true when current session has active stream
  const sessionId = ref<number | null>(null)
  const currentGroupChatId = ref<number | null>(null)
  const pendingAsk = ref<{ id: string; questions?: AskPayload['questions'] } | null>(null)
  const toolEvents = ref<ToolEvent[]>([])
  const tokenUsage = ref<TokenUsage>({ estimated_tokens: 0, context_window: 204800, usage_pct: 0 })
  const cacheUsage = ref({ hit: 0, miss: 0, ratio: 0 })
  const hasMoreOlder = ref(false)  // true if DB has more messages than loaded
  const loadingMore = ref(false)   // true while loading older messages
  const MESSAGE_PAGE_SIZE = 100

  // Per-session loading state
  const loadingSessions = new Set<number>()
  const { updateStreamState, clearStreamState, setupListener, teardownListener, clearAgentStreams } = useGlobalStreaming()

  let agentInvokedUnlisten: UnlistenFn | null = null
  let askUnlisten: UnlistenFn | null = null

  // Listen for ask_choice events (only for our session, ensuring agent+group-chat isolation)
  async function setupAskListener() {
    askUnlisten = await listen<AskPayload>('ask_choice', async (event) => {
      const { id, session_id, questions } = event.payload
      // Only show if this ask is for our session
      if (session_id !== sessionId.value) return
      pendingAsk.value = { id, questions }
    })
  }

  // Listen for being invoked by other agents via send_to_agent
  async function setupAgentInvokedListener() {
    agentInvokedUnlisten = await listen<AgentInvokedPayload>('agent_invoked', async (event) => {
      const { target_agent, session_id, group_chat_id } = event.payload
      if (target_agent !== agentType) return

      // Skip if already on this session (prevents duplicate DB load)
      if (sessionId.value === session_id) return

      console.log('[agent_invoked]', agentType, 'invoked, switching to session:', session_id, 'group:', group_chat_id)

      currentGroupChatId.value = group_chat_id ?? null
      sessionId.value = session_id

      await loadMessages()
    })
  }

  onMounted(() => {
    setupAgentInvokedListener()
    setupAskListener()
  })
  onUnmounted(() => {
    clearAgentStreams()
    if (agentInvokedUnlisten) {
      agentInvokedUnlisten()
    }
    if (askUnlisten) {
      askUnlisten()
    }
  })

  async function initSession(groupChatId: number) {
    if (groupChatId < 0) {
      // Save current messages before switching to temp chat
      if (sessionId.value) sessionMessages.set(sessionId.value, [...messages.value])
      currentGroupChatId.value = groupChatId
      sessionId.value = null
      messages.value = []
      hasMoreOlder.value = false
      tokenUsage.value = { estimated_tokens: 0, context_window: 204800, usage_pct: 0 }
      loading.value = false
      return
    }

    const prevSessionId = sessionId.value
    currentGroupChatId.value = groupChatId

    const sessions = await db.getAgentSessions(groupChatId, agentType)
    const session = sessions.find(s => s.agent_type === agentType)

    const newSessionId = session ? session.id : await db.createAgentSession(groupChatId, agentType)

    // Cache current messages and token usage before switching
    if (prevSessionId !== null && prevSessionId !== newSessionId) {
      sessionMessages.set(prevSessionId, [...messages.value])
      // Keep the old stream listener alive — background sessions continue running
    }

    sessionId.value = newSessionId

    // Restore from cache or load from DB
    const cached = sessionMessages.get(newSessionId)
    if (cached) {
      messages.value = cached
      const tu = sessionTokenUsage.get(newSessionId)
      if (tu) tokenUsage.value = tu
      loading.value = loadingSessions.has(newSessionId)
      hasMoreOlder.value = (sessionMeta.get(newSessionId)?.count ?? 0) > cached.length
    } else {
      await loadMessages()
    }
  }

  async function loadMessages(targetSessionId?: number) {
    const sid = targetSessionId ?? sessionId.value
    if (!sid) {
      tokenUsage.value = { estimated_tokens: 0, context_window: 204800, usage_pct: 0 }
      hasMoreOlder.value = false
      return
    }
    // Load total count + newest page in parallel
    const [totalCount, msgs] = await Promise.all([
      db.getMessageCount(sid).catch(() => 0),
      db.getMessages(sid, 0, MESSAGE_PAGE_SIZE)
    ])
    sessionMessages.set(sid, msgs)
    sessionMeta.set(sid, { count: totalCount })
    // Only update reactive state if loading for the currently viewed session
    if (sid === sessionId.value) {
      messages.value = msgs
      hasMoreOlder.value = msgs.length < totalCount
      const cached = sessionTokenUsage.get(sid)
      if (cached) {
        tokenUsage.value = cached
      } else if (msgs.length > 0) {
        const totalChars = msgs.reduce((sum, m) => sum + (m.content?.length || 0) + (m.thinking?.length || 0), 0)
        const est = Math.max(1, Math.round(totalChars / 3))
        const cw = tokenUsage.value.context_window
        tokenUsage.value = { estimated_tokens: est, context_window: cw, usage_pct: Math.min((est / cw) * 100, 99) }
      } else {
        tokenUsage.value = { estimated_tokens: 0, context_window: 204800, usage_pct: 0 }
      }
    }
    // Always update token cache for the loaded session
    const tu = sessionTokenUsage.get(sid)
    if (!tu && msgs.length > 0) {
      const totalChars = msgs.reduce((sum, m) => sum + (m.content?.length || 0) + (m.thinking?.length || 0), 0)
      const est = Math.max(1, Math.round(totalChars / 3))
      sessionTokenUsage.set(sid, { estimated_tokens: est, context_window: 204800, usage_pct: 0 })
    }
  }

  async function loadMoreMessages() {
    const sid = sessionId.value
    if (!sid || !hasMoreOlder.value || loadingMore.value) return
    loadingMore.value = true
    try {
      const currentLoaded = messages.value.length
      const olderMsgs = await db.getMessages(sid, currentLoaded, MESSAGE_PAGE_SIZE)
      if (olderMsgs.length > 0) {
        // Prepend older messages at the beginning
        messages.value = [...olderMsgs, ...messages.value]
        sessionMessages.set(sid, [...olderMsgs, ...(sessionMessages.get(sid) || [])])
      }
      const meta = sessionMeta.get(sid)
      hasMoreOlder.value = messages.value.length < (meta?.count ?? 0)
    } catch (e) {
      console.error('Failed to load more messages:', e)
    } finally {
      loadingMore.value = false
    }
  }

  // 构建发送给后端的历史消息（符合 MiniMax API 格式）
  interface HistoryMessage {
    role: string
    content: string
    tool_calls?: string
    thinking?: string
    raw_json?: string
  }

  function buildHistoryMessages(): HistoryMessage[] {
    const history: HistoryMessage[] = []

    for (const msg of messages.value) {
      if (msg.role === 'user') {
        history.push({
          role: 'user',
          content: msg.content,
          raw_json: msg.raw_json || undefined
        })
      } else if (msg.role === 'assistant') {
        history.push({
          role: 'assistant',
          content: msg.content || '',
          tool_calls: msg.tool_calls || undefined,
          thinking: msg.thinking || undefined,
          raw_json: msg.raw_json || undefined
        })
      } else if (msg.role === 'tool' || msg.raw_json) {
        // Tool result messages (stored as user role with tool_result content blocks)
        history.push({
          role: 'user',
          content: msg.content,
          raw_json: msg.raw_json || undefined
        })
      }
    }

    return history
  }

  async function sendMessage(content: string, attachments?: string, displayContent?: string, skipUserSave?: boolean) {
    console.log('[sendMessage] Starting with:', { agentType, currentGroupChatId: currentGroupChatId.value, sessionId: sessionId.value })

    // Handle temporary chat - create real chat in DB when first message is sent
    if (!currentGroupChatId.value || currentGroupChatId.value < 0) {
      const mode = agentType === 'ace' ? 'ace' : 'team'
      const chatName = content.slice(0, 10).replace(/[^一-龥a-zA-Z0-9]/g, '') || (mode === 'ace' ? 'Ace 对话' : '新群聊')
      console.log('[sendMessage] Creating new group chat:', chatName, 'mode:', mode)
      const realId = await db.createGroupChat(chatName, mode)
      currentGroupChatId.value = realId
      console.log('[sendMessage] Group chat created:', realId)
    }
    if (!sessionId.value) {
      console.log('[sendMessage] Creating new agent session for:', currentGroupChatId.value, agentType)
      sessionId.value = await db.createAgentSession(currentGroupChatId.value, agentType)
      console.log('[sendMessage] Agent session created:', sessionId.value)
    }

    const finalSessionId = sessionId.value
    console.log('[sendMessage] Final sessionId:', finalSessionId)

    // Save user message (skip if already saved by image flow)
    const display = displayContent || content
    if (!skipUserSave) {
      await db.addMessage(finalSessionId, 'user', display, undefined, undefined, attachments)
      messages.value.push({
        id: Date.now(),
        session_id: finalSessionId,
        role: 'user',
        content: display,
        attachments,
        created_at: new Date().toISOString(),
      } as ChatMessage)
    }

    loading.value = true
    loadingSessions.add(finalSessionId)
    toolEvents.value = []
    cacheUsage.value = { hit: 0, miss: 0, ratio: 0 }

    console.log('[sendMessage] Loading set to true, setting up event listener for:', `agent_stream_${finalSessionId}`)

    // Get workspace
    let workspace: string | null = null
    try {
      workspace = await invoke<string | null>('get_workspace')
    } catch (e) {
      console.warn('Could not get workspace:', e)
    }

    // Build message history for API
    const history = buildHistoryMessages()
    // If content has extra context not in display (e.g. vision results), inject it
    if (displayContent && content !== displayContent) {
      // Replace the last user message content with the full context
      for (let i = history.length - 1; i >= 0; i--) {
        if (history[i].role === 'user') {
          history[i].content = content
          break
        }
      }
    }
    console.log('[sendMessage] history:', JSON.stringify(history, null, 2))

    // 添加工具结果到历史消息（tool_result 需要紧跟 assistant 消息后）
    if (toolEvents.value.length > 0) {
      for (const tool of toolEvents.value) {
        if (tool.type === 'tool_end') {
          history.push({
            role: 'user',
            content: `Tool result for ${tool.tool}: ${tool.result || 'ok'}`
          })
        }
      }
    }

    // Clear and prepare stream state, then wire abort callback
    clearStreamState(finalSessionId)
    updateStreamState(finalSessionId, {
      text: '',
      thinking: '',
      done: false,
      toolEvents: [],
      abort: async () => {
        console.log('[abort] Aborting stream for session:', finalSessionId)
        try {
          await invoke('abort_stream', { sessionId: finalSessionId })
        } catch (e) {
          console.error('[abort] Failed to abort stream:', e)
        }
      },
      toolCallCount: 0
    })
    let fullText = ''
    let fullThinking = ''
    let toolCallCount = 0
    const streamToolEvents: Array<{ type: 'tool_start' | 'tool_end'; tool: string; tool_id: string; input?: Record<string, any>; result?: string }> = []
    // 收集 tool_calls 信息
    const collectedToolCalls: Array<{
      id: string
      type: 'function'
      function: { name: string; arguments: string }
    }> = []

    // Promise-based completion — replaces the polling loop
    let resolveStream: (() => void) | null = null
    const streamComplete = new Promise<void>(r => { resolveStream = r })

    // Set up event listener for real-time streaming (managed by useGlobalStreaming)
    console.log('[sendMessage] Setting up event listener for:', `agent_stream_${finalSessionId}`)
    await setupListener(finalSessionId, (event) => {
      console.log('[sendMessage] Received event:', event.payload.type)
      const ev = event.payload
      switch (ev.type) {
        case 'content_block_delta':
          if (ev.content) fullText += ev.content
          if (ev.thinking) fullThinking += ev.thinking
          if (ev.content || ev.thinking) {
            updateStreamState(finalSessionId, {
              text: fullText, thinking: fullThinking, done: false, toolCallCount
            })
          }
          break
        case 'tool_start':
          toolCallCount++
          if (ev.tool_id && ev.tool) {
            collectedToolCalls.push({
              id: ev.tool_id, type: 'function',
              function: { name: ev.tool, arguments: JSON.stringify(ev.input || {}) }
            })
          }
          {
            const te = { type: 'tool_start' as const, tool: ev.tool || '', tool_id: ev.tool_id || '', input: ev.input, textBefore: fullText }
            toolEvents.value.push(te)
            streamToolEvents.push(te)
            updateStreamState(finalSessionId, {
              text: fullText, thinking: fullThinking, done: false, toolCallCount, toolEvents: [...streamToolEvents]
            })
          }
          break
        case 'tool_end':
          {
            const te = { type: 'tool_end' as const, tool: ev.tool || '', tool_id: ev.tool_id || '', result: ev.result }
            toolEvents.value.push(te)
            streamToolEvents.push(te)
            updateStreamState(finalSessionId, {
              text: fullText, thinking: fullThinking, done: false, toolCallCount, toolEvents: [...streamToolEvents]
            })
          }
          break
        case 'done':
          resolveStream?.()
          updateStreamState(finalSessionId, {
            text: fullText, thinking: fullThinking, done: false, toolCallCount
          })
          break
        case 'aborted':
          resolveStream?.()
          updateStreamState(finalSessionId, {
            text: fullText, thinking: fullThinking, done: false, toolCallCount
          })
          break
        case 'cache_usage':
          cacheUsage.value = {
            hit: ev.cache_hit_tokens || 0,
            miss: ev.cache_miss_tokens || 0,
            ratio: Math.round((ev.cache_hit_ratio || 0) * 100),
          }
          break
        case 'token_usage':
          // Never let the displayed token count decrease within a session.
          const cached = sessionTokenUsage.get(finalSessionId)
          const est = ev.estimated_tokens || 0
          const tu = {
            estimated_tokens: cached && cached.estimated_tokens > est ? cached.estimated_tokens : est,
            context_window: ev.context_window || 204800,
            usage_pct: ev.usage_pct || 0
          }
          sessionTokenUsage.set(finalSessionId, tu)
          // Only update reactive display if user is viewing this session
          if (finalSessionId === sessionId.value) {
            tokenUsage.value = tu
          }
          break
        case 'error':
          resolveStream?.()
          fullText += `\n\nError: ${ev.content}`
          updateStreamState(finalSessionId, {
            text: fullText,
            thinking: fullThinking,
            done: true,
            toolCallCount
          })
          break
      }
    })

    // Snapshot message count before the stream — used to detect whether
    // backend persisted new messages after the stream completes.
    const msgCountBefore = messages.value.length

    try {
      // Call Rust agent service
      console.log('[sendMessage] Invoking agent_chat_stream with:', { agentType, historyLength: history.length, sessionId: finalSessionId })
      await invoke('agent_chat_stream', {
        agentType,
        messages: JSON.stringify(history),
        system: null,
        workspace,
        sessionId: finalSessionId
      })
      console.log('[sendMessage] invoke completed')

      // Wait for stream to complete (event-driven, no polling)
      console.log('[sendMessage] Waiting for stream...')
      await streamComplete

      // Keep streaming content visible until DB load confirms persistence
      updateStreamState(finalSessionId, {
        text: fullText || undefined,
        thinking: fullThinking,
        done: false,
        toolCallCount
      })

      // Reload messages for this specific session (not current view)
      await loadMessages(finalSessionId)

      // Clear streaming display after DB messages are available
      updateStreamState(finalSessionId, {
        text: '', thinking: '', done: true, toolCallCount
      })

      // Fallback: if no new messages appeared in DB, push stream buffer
      // into the session cache so the user sees output even after tab switches.
      if (fullText || fullThinking) {
        const cachedMsgs = sessionMessages.get(finalSessionId) || []
        if (cachedMsgs.length <= msgCountBefore) {
          const fallbackContent = fullThinking
            ? `💭 ${fullThinking}\n\n${fullText}`
            : fullText
          const fallbackMsg = {
            id: Date.now() + 1,
            session_id: finalSessionId,
            role: 'assistant',
            content: fallbackContent,
            thinking: fullThinking || undefined,
            tool_calls: collectedToolCalls.length > 0 ? JSON.stringify(collectedToolCalls) : undefined,
            created_at: new Date().toISOString(),
          } as ChatMessage
          cachedMsgs.push(fallbackMsg)
          sessionMessages.set(finalSessionId, cachedMsgs)
          // If user is still viewing this session, update the reactive array too
          if (finalSessionId === sessionId.value) {
            messages.value.push(fallbackMsg)
          }
        }
      }

      toolEvents.value = []

    } catch (e: any) {
      console.error('Agent error:', e)
      const errorMsg = `Error: ${e.toString()}`
      updateStreamState(finalSessionId, {
        text: errorMsg,
        done: true,
        toolCallCount
      })
      await db.addMessage(finalSessionId, 'assistant', errorMsg)
      await loadMessages(finalSessionId)
    } finally {
      loadingSessions.delete(finalSessionId)
      // Only show loading state if the current session is still streaming
      loading.value = loadingSessions.has(sessionId.value!)
      teardownListener(finalSessionId)
    }
  }

  function clearToolEvents() {
    toolEvents.value = []
  }

  function clearAsk() {
    pendingAsk.value = null
  }

  async function switchGroupChat(groupChatId: number) {
    await initSession(groupChatId)
  }

  // ---- Retry / Rewind / Clear ----

  async function retryMessage(messageIndex: number) {
    const msg = messages.value[messageIndex]
    if (!msg || msg.role !== 'user') return
    const content = msg.content
    const att = msg.attachments
    await sendMessage(content, att)
  }

  const showRewindConfirm = ref<{ messageId: number; content: string } | null>(null)

  function rewindToMessage(messageId: number, content: string) {
    showRewindConfirm.value = { messageId, content }
  }

  async function confirmRewind(): Promise<string | null> {
    const info = showRewindConfirm.value
    if (!info || !sessionId.value) return null
    try {
      clearStreamState(sessionId.value)
      const savedContent = await invoke<string>('rewind_conversation', {
        sessionId: sessionId.value,
        messageId: info.messageId,
      })
      await loadMessages()
      showRewindConfirm.value = null
      return savedContent
    } catch (e) {
      console.error('Rewind failed:', e)
      showRewindConfirm.value = null
      return null
    }
  }

  function cancelRewind() {
    showRewindConfirm.value = null
  }

  const showClearConfirm = ref(false)

  async function clearConversation() {
    if (!sessionId.value) return
    try {
      clearStreamState(sessionId.value)
      await db.clearSessionHistory(sessionId.value)
      messages.value = []
      sessionMessages.delete(sessionId.value)
      toolEvents.value = []
      tokenUsage.value = { estimated_tokens: 0, context_window: 204800, usage_pct: 0 }
      sessionTokenUsage.delete(sessionId.value)
      showClearConfirm.value = false
    } catch (e) {
      console.error('Clear failed:', e)
    }
  }

  interface ContentBlock {
    type: string
    text?: string
    tool_use_id?: string
    content?: string | unknown
    tool_use?: unknown
    thinking?: unknown
  }

  function isInternalCacheMessage(msg: ChatMessage): boolean {
    if (msg.role === 'user' && msg.content?.startsWith('## 内置参考资料')) return true
    if (msg.raw_json) {
      try {
        const blocks: ContentBlock[] = JSON.parse(msg.raw_json)
        if (Array.isArray(blocks)) {
          if (msg.role === 'user' && blocks.some((b) => b.type === 'tool_result')) return true
          if (msg.role === 'assistant' && blocks.some((b) => b.type === 'tool_use')) {
            const hasText = blocks.some((b) => b.type === 'text' && (b.text ?? '').trim())
            const hasThinking = blocks.some((b) => b.type === 'thinking')
            if (!hasText && !hasThinking) return true
          }
        }
      } catch {}
    }
    if (msg.role === 'user' && !msg.content?.trim()) return true
    if (msg.tool_calls) {
      try {
        const tc = JSON.parse(msg.tool_calls)
        if (Array.isArray(tc) && tc.some((t: any) =>
          ['skill', 'match_skills', 'list_skills'].includes(t.function?.name)
        )) return true
      } catch {}
    }
    return false
  }

  const displayMessages = computed(() => {
    // Collect tool results from hidden tool_result messages
    const toolResults = new Map<string, string>()
    for (const m of messages.value) {
      if (m.raw_json) {
        try {
          const blocks = JSON.parse(m.raw_json)
          if (Array.isArray(blocks)) {
            for (const b of blocks) {
              if (b.type === 'tool_result' && b.tool_use_id && b.content) {
                toolResults.set(
                  b.tool_use_id,
                  typeof b.content === 'string' ? b.content : JSON.stringify(b.content)
                )
              }
            }
          }
        } catch {}
      }
    }

    const result: UIMessage[] = []
    for (const m of messages.value) {
      const hidden = isInternalCacheMessage(m)
      if (!hidden) {
        if (m.content && m.content.startsWith('💭')) {
          const parts = m.content.split('\n\n')
          result.push({
            ...m,
            thinking: parts[0].replace('💭 ', ''),
            content: parts.slice(1).join('\n\n'),
          })
        } else {
          result.push({ ...m, thinking: m.thinking } as UIMessage)
        }
      }
      // Emit tool cards after this message (whether hidden or not)
      if (m.role === 'assistant' && m.tool_calls) {
        try {
          const calls = JSON.parse(m.tool_calls)
          if (Array.isArray(calls)) {
            for (const tc of calls) {
              result.push({
                id: `tool-${tc.id}` as unknown as number,
                session_id: m.session_id,
                role: 'tool' as const,
                tool_calls: JSON.stringify([tc]),
                content: toolResults.get(tc.id) || '',
                created_at: m.created_at,
                thinking: undefined,
                attachments: undefined,
                raw_json: undefined,
              } as UIMessage)
            }
          }
        } catch {}
      }
    }
    return result
  })

  return {
    messages,
    loading,
    sessionId,
    currentGroupChatId,
    pendingAsk,
    toolEvents,
    displayMessages,
    initSession,
    loadMessages,
    loadMoreMessages,
    hasMoreOlder,
    loadingMore,
    sendMessage,
    clearAsk,
    clearToolEvents,
    switchGroupChat,
    tokenUsage,
    cacheUsage,
    retryMessage,
    showRewindConfirm,
    rewindToMessage,
    confirmRewind,
    cancelRewind,
    showClearConfirm,
    clearConversation,
  }
}
