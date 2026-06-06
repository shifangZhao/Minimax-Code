// Agent conversation using Rust backend via Tauri invoke
// Supports Interleaved Thinking with complete message history

import { ref, computed, onMounted, onUnmounted } from 'vue'
import { db, type ChatMessage, type MessagePart, type UIMessage } from '../services/db'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useGlobalStreaming, activeFrontendSessions } from './useGlobalStreaming'
import { processCommandEvent, markOrphan } from './useCommandStream'
import { useTodoStore } from './useTodoStore'
import type { AskQuestion } from '../types/api'

export interface ToolEvent {
  type: 'tool_start' | 'tool_end'
  tool: string
  tool_id: string
  input?: Record<string, unknown>
  result?: string
  thinkingBefore?: string
}

export interface AskPayload {
  id: string
  session_id: number
  agent_type?: string
  question?: string
  questions?: AskQuestion[]
  options?: string[]
}

export interface AgentInvokedPayload {
  target_agent: string
  session_id: number
  group_chat_id?: number
  message?: string
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
const sessionCacheUsage = new Map<number, { hit: number; miss: number; ratio: number }>()
const sessionMessages = new Map<number, ChatMessage[]>()
const sessionMeta = new Map<number, { count: number }>()
const sessionDuration = new Map<number, number>()

// Memory optimization: limit cached sessions to prevent memory leaks
const MAX_CACHED_SESSIONS = 3        // Reduced from 5 - only keep current + 2 recent
const MAX_MESSAGES_PER_SESSION = 300  // Reduced from 500

// LRU cache management for sessionMessages
function evictOldSessions() {
  if (sessionMessages.size <= MAX_CACHED_SESSIONS) return
  
  // Get all session IDs and remove the oldest ones
  const sessionIds = [...sessionMessages.keys()]
  const toRemove = sessionIds.slice(0, sessionIds.length - MAX_CACHED_SESSIONS)
  
  for (const id of toRemove) {
    sessionMessages.delete(id)
    sessionTokenUsage.delete(id)
    sessionCacheUsage.delete(id)
    sessionMeta.delete(id)
  }
}

// Trim messages to limit memory usage - keep user messages + recent
function trimMessages(messages: ChatMessage[]): ChatMessage[] {
  if (messages.length <= MAX_MESSAGES_PER_SESSION) return messages
  
  // Keep last N messages
  return messages.slice(-MAX_MESSAGES_PER_SESSION)
}

// Periodic sweep: clean stale caches every 2 min
let _sweepTimer: ReturnType<typeof setInterval> | null = null
function ensureCacheSweep() {
  if (_sweepTimer) return
  _sweepTimer = setInterval(() => {
    // Clear caches for sessions that have no active listeners
    for (const id of sessionMessages.keys()) {
      if (!activeFrontendSessions.has(id)) {
        // Keep for one more cycle, then evict
      }
    }
    evictOldSessions()
  }, 120_000)
}

// User's currently configured context window, in tokens.
// Written by SettingsPanel after load/save; also refreshed by the token_usage
// event from the backend (authoritative). Used as the default for places that
// used to hardcode 204800, so the progress bar reflects the real setting.
let userContextWindow = 0

export function setUserContextWindow(cw: number) {
  if (cw > 0) userContextWindow = cw
}

function defaultContextWindow(): number {
  return userContextWindow
}

export function useAgentConversation(agentType: string, options?: { onEditsChanged?: (sessionId: number) => void | Promise<void> }) {
  const onEditsChanged = options?.onEditsChanged
  const messages = ref<ChatMessage[]>([])
  const loading = ref(false)  // true when current session has active stream
  const sessionId = ref<number | null>(null)
  const currentGroupChatId = ref<number | null>(null)
  const pendingAsk = ref<{ id: string; questions?: AskPayload['questions'] } | null>(null)
  const toolEvents = ref<ToolEvent[]>([])
  const tokenUsage = ref<TokenUsage>({ estimated_tokens: 0, context_window: defaultContextWindow(), usage_pct: 0 })
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
      // 防御：questions 不是非空数组时不显示（避免渲染出空卡片）
      if (!Array.isArray(questions) || questions.length === 0) {
        console.warn('[ask_choice] received empty/invalid questions payload:', event.payload)
        return
      }
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

      currentGroupChatId.value = group_chat_id ?? null
      sessionId.value = session_id

      await loadMessages()
    })
  }

  onMounted(() => {
    setupAgentInvokedListener()
    setupAskListener()
    ensureCacheSweep()
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
      if (sessionId.value) {
        sessionMessages.set(sessionId.value, trimMessages([...messages.value]))
        sessionTokenUsage.set(sessionId.value, { ...tokenUsage.value })
        sessionCacheUsage.set(sessionId.value, { ...cacheUsage.value })
        evictOldSessions()
      }
      currentGroupChatId.value = groupChatId
      sessionId.value = null
      messages.value = []
      hasMoreOlder.value = false
      tokenUsage.value = { estimated_tokens: 0, context_window: defaultContextWindow(), usage_pct: 0 }
      cacheUsage.value = { hit: 0, miss: 0, ratio: 0 }
      loading.value = false
      return
    }

    const prevSessionId = sessionId.value
    currentGroupChatId.value = groupChatId

    const sessions = await db.getAgentSessions(groupChatId, agentType)
    const session = sessions.find(s => s.agent_type === agentType)

    const newSessionId = session ? session.id : await db.createAgentSession(groupChatId, agentType)

    // Cache current messages, token usage and cache usage before switching
    if (prevSessionId !== null && prevSessionId !== newSessionId) {
      sessionMessages.set(prevSessionId, trimMessages([...messages.value]))
      sessionTokenUsage.set(prevSessionId, { ...tokenUsage.value })
      sessionCacheUsage.set(prevSessionId, { ...cacheUsage.value })
      // Keep the old stream listener alive — background sessions continue running
      evictOldSessions()

      // Clear display state immediately to avoid flashing old content
      messages.value = []
      tokenUsage.value = { estimated_tokens: 0, context_window: defaultContextWindow(), usage_pct: 0 }
      cacheUsage.value = { hit: 0, miss: 0, ratio: 0 }
      loading.value = false
      hasMoreOlder.value = false
    }

    sessionId.value = newSessionId

    // Restore from cache or load from DB
    const cached = sessionMessages.get(newSessionId)
    if (cached) {
      messages.value = cached
      const tu = sessionTokenUsage.get(newSessionId)
      if (tu) tokenUsage.value = tu
      const cu = sessionCacheUsage.get(newSessionId)
      cacheUsage.value = cu ?? { hit: 0, miss: 0, ratio: 0 }
      loading.value = loadingSessions.has(newSessionId)
      hasMoreOlder.value = (sessionMeta.get(newSessionId)?.count ?? 0) > cached.length
    } else {
      // Use the API-reported token usage from the session if available
      const sessionTokenCount = session?.last_token_usage || 0
      if (sessionTokenCount > 0) {
        const cw = defaultContextWindow() || tokenUsage.value.context_window
        tokenUsage.value = {
          estimated_tokens: sessionTokenCount,
          context_window: cw,
          usage_pct: cw > 0 ? Math.min((sessionTokenCount / cw) * 100, 99) : 0
        }
        sessionTokenUsage.set(newSessionId, tokenUsage.value)
      }
      await loadMessages()
    }
  }

  async function loadMessages(targetSessionId?: number) {
    const sid = targetSessionId ?? sessionId.value
    if (!sid) {
      tokenUsage.value = { estimated_tokens: 0, context_window: defaultContextWindow(), usage_pct: 0 }
      hasMoreOlder.value = false
      return
    }
    // Load total count + newest page in parallel
    const [totalCount, msgs] = await Promise.all([
      db.getMessageCount(sid).catch(() => 0),
      db.getMessages(sid, 0, MESSAGE_PAGE_SIZE)
    ])
    sessionMessages.set(sid, trimMessages(msgs))
    sessionMeta.set(sid, { count: totalCount })
    evictOldSessions()
    // Only update reactive state if loading for the currently viewed session
    if (sid === sessionId.value) {
      messages.value = msgs
      hasMoreOlder.value = msgs.length < totalCount
      const cached = sessionTokenUsage.get(sid)
      if (cached) {
        tokenUsage.value = cached
      }
      // If no cached token usage, keep the current value to avoid flashing to 0.
      // The backend will send a token_usage event with the real count shortly.
      // Restore per-session cache usage
      const cu = sessionCacheUsage.get(sid)
      cacheUsage.value = cu ?? { hit: 0, miss: 0, ratio: 0 }
    }
    // Always update token cache for the loaded session if not already cached
    const tu = sessionTokenUsage.get(sid)
    if (!tu && msgs.length > 0) {
      sessionTokenUsage.set(sid, { estimated_tokens: 0, context_window: defaultContextWindow(), usage_pct: 0 })
    }

    // Restore todo panel state from persisted messages
    const allParts = msgs.flatMap(m => m.parts || [])
    useTodoStore().restoreFromMessages(sid, allParts)
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
        sessionMessages.set(sid, trimMessages([...olderMsgs, ...(sessionMessages.get(sid) || [])]))
      }
      const meta = sessionMeta.get(sid)
      hasMoreOlder.value = messages.value.length < (meta?.count ?? 0)
    } catch (e) {
      console.error('Failed to load more messages:', e)
    } finally {
      loadingMore.value = false
    }
  }

  async function sendMessage(content: string, attachments?: string, displayContent?: string) {
    // Handle temporary chat - create real chat in DB when first message is sent
    if (!currentGroupChatId.value || currentGroupChatId.value < 0) {
      const chatName = content.slice(0, 10).replace(/[^一-龥a-zA-Z0-9]/g, '') || 'Ace 对话'
      const realId = await db.createGroupChat(chatName, 'ace')
      currentGroupChatId.value = realId
    }
    if (!sessionId.value) {
      sessionId.value = await db.createAgentSession(currentGroupChatId.value, agentType)
    }

    const finalSessionId = sessionId.value

    // Save user message to DB immediately, then optimistic push for instant UI
    const display = displayContent || content
    const userMsgId = await db.addMessage(finalSessionId, 'user', display, undefined, undefined, attachments)
    messages.value.push({
      id: userMsgId,
      session_id: finalSessionId,
      role: 'user',
      content: display,
      attachments,
      created_at: new Date().toISOString(),
    } as ChatMessage)

    loading.value = true
    loadingSessions.add(finalSessionId)
    toolEvents.value = []
    cacheUsage.value = { hit: 0, miss: 0, ratio: 0 }
    const startTime = Date.now()

    // Get workspace — prefer per-session (group_chat), fallback to global
    let workspace: string | null = null
    try {
      workspace = await invoke<string | null>('get_group_chat_workspace', { groupChatId: currentGroupChatId.value })
      if (!workspace) {
        workspace = await invoke<string | null>('get_workspace')
      }
    } catch (e) {
      console.warn('Could not get workspace:', e)
    }

    // Clear and prepare stream state, then wire abort callback
    clearStreamState(finalSessionId)
    updateStreamState(finalSessionId, {
      text: '',
      thinking: '',
      done: false,
      started: false,
      toolEvents: [],
      abort: async () => {
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
    const streamToolEvents: Array<{
      type: 'tool_start' | 'tool_end'
      tool: string
      tool_id: string
      input?: Record<string, unknown>
      result?: string
      // Snapshot of the cumulative text/thinking at the moment the tool
      // started — lets the optimistic message reconstruct the LLM's
      // original interleaving (text → tool → text → tool → text)
      // instead of dumping all tool cards below the final text.
      textBefore?: string
      thinkingBefore?: string
    }> = []
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
    activeFrontendSessions.add(finalSessionId)

    // rAF pacing: buffer rapid stream events and flush once per animation frame.
    // This is the sole throttle — updateStreamState is synchronous (new Map per call).
    let rAFPending = false
    const flushRAF = () => {
      rAFPending = false
      updateStreamState(finalSessionId, {
        text: fullText, thinking: fullThinking, done: false, started: true, toolCallCount
      })
    }

    await setupListener(finalSessionId, (event) => {
      const ev = event.payload
      switch (ev.type) {
        case 'content_block_delta':
          if (ev.content) fullText += ev.content
          if (ev.thinking) fullThinking += ev.thinking
          if (ev.content || ev.thinking) {
            if (!rAFPending) {
              rAFPending = true
              requestAnimationFrame(flushRAF)
            }
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
            const te = { type: 'tool_start' as const, tool: ev.tool || '', tool_id: ev.tool_id || '', input: ev.input, textBefore: fullText, thinkingBefore: fullThinking }
            toolEvents.value.push(te)
            streamToolEvents.push(te)
            // tool_start is a discrete event — update state immediately so
            // streamingSegments can interleave the tool card at the correct
            // position in the text flow. Only text deltas need rAF pacing.
            updateStreamState(finalSessionId, {
              text: fullText, thinking: fullThinking, done: false, started: true, toolCallCount, toolEvents: [...streamToolEvents]
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
            // Update todo panel when todo_write completes
            if (ev.tool === 'todo_write' && ev.result) {
              useTodoStore().updateFromResult(finalSessionId, ev.result)
            }
            // Snapshot was just saved before the tool ran — refresh the edits
            // list so the undo button reflects the latest file modification
            // (otherwise the user sees a stale list and clicks fail).
            if (onEditsChanged) {
              Promise.resolve(onEditsChanged(finalSessionId)).catch((e) =>
                console.warn('[useAgentConversation] onEditsChanged failed:', e)
              )
            }
          }
          break
        case 'abort_acknowledged':
          // Backend acknowledged abort — freeze display immediately
          updateStreamState(finalSessionId, {
            text: fullText, thinking: fullThinking, done: true, toolCallCount
          })
          break
        case 'done':
          // Flush any pending rAF before completing
          if (rAFPending) { rAFPending = false; flushRAF() }
          resolveStream?.()
          // Refresh edits one final time after stream completes — covers
          // any tool_result persistence that happened in the final turn.
          if (onEditsChanged) {
            Promise.resolve(onEditsChanged(finalSessionId)).catch((e) =>
              console.warn('[useAgentConversation] onEditsChanged failed:', e)
            )
          }
          break
        case 'aborted':
          if (rAFPending) { rAFPending = false; flushRAF() }
          resolveStream?.()
          // User paused/aborted — refresh edits so the undo button is usable
          // immediately (don't wait for the post-stream loadMessages).
          if (onEditsChanged) {
            Promise.resolve(onEditsChanged(finalSessionId)).catch((e) =>
              console.warn('[useAgentConversation] onEditsChanged failed:', e)
            )
          }
          break
        case 'cache_usage':
          // Only update reactive display if user is viewing this session
          if (finalSessionId === sessionId.value) {
            cacheUsage.value = {
              hit: ev.cache_hit_tokens || 0,
              miss: ev.cache_miss_tokens || 0,
              ratio: (ev.cache_hit_ratio || 0) * 100,
            }
          }
          break
        case 'token_usage':
          // Never let the displayed token count decrease within a session.
          const cached = sessionTokenUsage.get(finalSessionId)
          const est = ev.estimated_tokens || 0
          if (ev.context_window && ev.context_window > 0) setUserContextWindow(ev.context_window)
          const cw = ev.context_window || userContextWindow
          const tu = {
            estimated_tokens: cached && cached.estimated_tokens > est ? cached.estimated_tokens : est,
            context_window: cw,
            usage_pct: cw > 0 ? (ev.usage_pct || (est / cw) * 100) : 0
          }
          sessionTokenUsage.set(finalSessionId, tu)
          // Only update reactive display if user is viewing this session
          if (finalSessionId === sessionId.value) {
            tokenUsage.value = tu
          }
          break
        case 'db_updated':
          // Real-time DB update: update or add the message in local cache
          // thinking goes to parts, text goes to content (separated for display)
          if (ev.message_id) {
            const existing = messages.value.find(m => m.id === ev.message_id)
            if (existing) {
              if (!existing.parts) existing.parts = []
              if (ev.thinking) {
                existing.parts.push({
                  id: Date.now(),
                  message_id: Number(existing.id),
                  session_id: existing.session_id,
                  part_order: existing.parts.length,
                  part_type: 'thinking' as const,
                  content: ev.thinking,
                  created_at: new Date().toISOString(),
                })
              }
              if (ev.content) {
                existing.content = (existing.content || '') + ev.content
              }
              existing.created_at = new Date().toISOString()
            }
          }
          // db_updated fires when tool_result is persisted to DB. Since the
          // file_snapshot was saved before the tool ran, the snapshot is in
          // DB by now — refresh the edits list so the undo button is ready.
          if (onEditsChanged) {
            Promise.resolve(onEditsChanged(finalSessionId)).catch((e) =>
              console.warn('[useAgentConversation] onEditsChanged failed:', e)
            )
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
        // Streaming command protocol (NDJSON-style). These events drive
        // the live ToolCard view; the final `tool_end` still carries the
        // aggregated result for the LLM. See useCommandStream + command_stream.rs.
        case 'exec_command_begin':
        case 'exec_command_output_delta':
        case 'exec_command_end':
        case 'exec_command_error':
          processCommandEvent(ev)
          break
      }
    })

    // Snapshot message count before the stream — used to detect whether
    // backend persisted new messages after the stream completes.
    const msgCountBefore = messages.value.length

    try {
      // Call Rust agent service — backend reads context from DB, appends new message
      await invoke('agent_chat_stream', {
        agentType,
        newMessage: content,
        attachments: attachments || null,
        system: null,
        workspace,
        sessionId: finalSessionId
      })

      // Wait for stream to complete (event-driven, no polling)
      await streamComplete

      // Build fallback message from stream buffer FIRST, before touching
      // streaming state. This guarantees something is visible immediately
      // in the persisted section — no blank gap.
      let optimisticMsg: ChatMessage | null = null
      let optimisticList: ChatMessage[] | null = null
      if (fullText || fullThinking) {
        // Build fallback parts in the LLM's original interleaving order
        // (thinking → text → tool → text → tool → text). tool_start events
        // carry a snapshot of fullText / fullThinking at the moment the
        // tool was called, so we can slice the cumulative strings at those
        // positions to recover the per-segment text/thinking each tool
        // sat between. Matches the DB layout produced by the backend
        // (which preserves the same order via OrderedBlock) so the UI is
        // consistent between the optimistic push and the post-stream
        // loadMessages overwrite.
        const fallbackParts: MessagePart[] = []
        let partOrder = 0
        const nowIso = new Date().toISOString()
        const pushPart = (part: Omit<MessagePart, 'id' | 'message_id' | 'session_id' | 'part_order' | 'created_at'>) => {
          fallbackParts.push({
            id: 0,
            message_id: 0,
            session_id: finalSessionId,
            part_order: partOrder++,
            created_at: nowIso,
            ...part,
          } as MessagePart)
        }
        // The order of streamToolEvents matches the LLM's generation
        // order; tool_start carries the textBefore/thinkingBefore hints
        // we slice from, tool_end is the same tool_id and just records
        // the result — we ignore tool_end here (the tool_use part is
        // already emitted at tool_start time).
        const toolStartEvents = streamToolEvents.filter(e => e.type === 'tool_start')
        if (toolStartEvents.length === 0) {
          // No tool calls — flat layout is fine and matches the DB.
          if (fullThinking) pushPart({ part_type: 'thinking', content: fullThinking })
          if (fullText) pushPart({ part_type: 'text', content: fullText })
        } else {
          let prevTextEnd = 0
          let prevThinkEnd = 0
          for (const ev of toolStartEvents) {
            const textBefore = ev.textBefore ?? fullText
            const thinkingBefore = ev.thinkingBefore ?? fullThinking
            const thinkSegment = fullThinking.slice(prevThinkEnd, thinkingBefore.length)
            if (thinkSegment) {
              pushPart({ part_type: 'thinking', content: thinkSegment })
            }
            const textSegment = fullText.slice(prevTextEnd, textBefore.length)
            if (textSegment) {
              pushPart({ part_type: 'text', content: textSegment })
            }
            const tc = collectedToolCalls.find(c => c.id === ev.tool_id)
            pushPart({
              part_type: 'tool_use',
              content: '',
              tool_use_id: ev.tool_id,
              tool_name: tc?.function.name ?? ev.tool,
              tool_input: tc?.function.arguments ?? JSON.stringify(ev.input ?? {}),
            })
            prevTextEnd = textBefore.length
            prevThinkEnd = thinkingBefore.length
          }
          // Trailing thinking / text after the last tool.
          const remThinking = fullThinking.slice(prevThinkEnd)
          if (remThinking) pushPart({ part_type: 'thinking', content: remThinking })
          const remText = fullText.slice(prevTextEnd)
          if (remText) pushPart({ part_type: 'text', content: remText })
        }
        const fallbackContent = fullThinking
          ? `💭 ${fullThinking}\n\n${fullText}`
          : fullText
        optimisticMsg = {
          id: Date.now() + 1,
          session_id: finalSessionId,
          role: 'assistant',
          content: fallbackContent,
          parts: fallbackParts,
          created_at: new Date().toISOString(),
        } as ChatMessage
        // Optimistic UI: push immediately so user sees the response.
        // loadMessages will silently overwrite with the real DB row afterwards.
        const cachedBefore = sessionMessages.get(finalSessionId) || []
        optimisticList = [...cachedBefore, optimisticMsg]
        sessionMessages.set(finalSessionId, trimMessages(optimisticList))
        if (finalSessionId === sessionId.value) {
          messages.value = optimisticList
        }
      }

      // Store duration for this session
      const durationMs = Date.now() - startTime
      sessionDuration.set(finalSessionId, durationMs)

      // Now hide streaming — the persisted section already has the message
      updateStreamState(finalSessionId, {
        text: '', thinking: '', done: true, toolCallCount
      })

      // Reload from DB to get authoritative message IDs and any parts the
      // backend saved (tool results, etc.). Overwrites the optimistic copy.
      await loadMessages(finalSessionId)

      // If DB still doesn't have the message (save failed), the optimistic
      // copy is already in the cache and visible — nothing more to do.
      // But if DB load brought back fewer messages than expected, re-apply.
      if (optimisticList) {
        const after = sessionMessages.get(finalSessionId) || []
        if (after.length <= msgCountBefore) {
          sessionMessages.set(finalSessionId, trimMessages(optimisticList))
          if (finalSessionId === sessionId.value) {
            messages.value = optimisticList
          }
        }
      }

      toolEvents.value = []

    } catch (e: unknown) {
      console.error('Agent error:', e)
      const errorMsg = `Error: ${e instanceof Error ? e.message : String(e)}`
      // Mark any in-flight command as orphan so its ToolCard exits the spinner
      for (const te of toolEvents.value) {
        if (te.type === 'tool_start' && (te.tool === 'run_command' || te.tool === 'run_background')) {
          markOrphan(te.tool_id)
        }
      }
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
      activeFrontendSessions.delete(finalSessionId)
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

  async function retryMessageById(msgId: number) {
    const msg = messages.value.find(m => m.id === msgId)
    if (!msg || msg.role !== 'user') return
    await sendMessage(msg.content, msg.attachments)
  }

  const showRewindConfirm = ref<{ dbId: number; content: string } | null>(null)

  function rewindToMessage(dbId: number, content: string) {
    showRewindConfirm.value = { dbId, content }
  }

  async function confirmRewind(): Promise<string | null> {
    const info = showRewindConfirm.value
    if (!info || !sessionId.value) return null
    try {
      clearStreamState(sessionId.value)
      const savedContent = await invoke<string>('rewind_conversation', {
        sessionId: sessionId.value,
        messageId: info.dbId,
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

  /** Clear module-level caches for a session without touching the DB. */
  function clearSessionCache(sid: number | null) {
    if (sid === null) return
    sessionMessages.delete(sid)
    sessionTokenUsage.delete(sid)
    sessionCacheUsage.delete(sid)
    sessionMeta.delete(sid)
    useTodoStore().clearState(sid)
  }

  async function clearConversation() {
    if (!sessionId.value) return
    try {
      clearStreamState(sessionId.value)
      await db.clearSessionHistory(sessionId.value)
      messages.value = []
      clearSessionCache(sessionId.value)
      toolEvents.value = []
      tokenUsage.value = { estimated_tokens: 0, context_window: defaultContextWindow(), usage_pct: 0 }
      cacheUsage.value = { hit: 0, miss: 0, ratio: 0 }
      showClearConfirm.value = false
    } catch (e) {
      console.error('Clear failed:', e)
    }
  }

  function isInternalCacheMessage(msg: ChatMessage): boolean {
    if (msg.role === 'user' && msg.content?.startsWith('## 内置技能')) return true
    if (msg.role === 'user' && !msg.content?.trim() && (!msg.parts || msg.parts.length === 0)) return true
    // Hide user messages that are purely tool_results (no text content, only tool_result parts)
    if (msg.role === 'user' && msg.parts && msg.parts.length > 0 && msg.parts.every(p => p.part_type === 'tool_result')) return true
    // Hide assistant messages that have zero displayable parts
    // (tool_use parts ARE displayable — they render as ToolCards)
    if (msg.role === 'assistant' && msg.parts && msg.parts.length > 0) {
      const hasVisible = msg.parts.some(p => p.part_type === 'text' || p.part_type === 'thinking' || p.part_type === 'tool_use')
      if (!hasVisible) return true
    }
    return false
  }

  // Cache fingerprints to avoid re-processing unchanged message arrays
  let _dispFingerprint = ''
  let _dispCache: UIMessage[] = []
  let _dispLastArray: ChatMessage[] | null = null
  const _contentLengths = new Map<string | number, number>()

  const displayMessages = computed(() => {
    // Structural fingerprint: id:role:parts.length (NOT content.length)
    const count = messages.value.length
    let structuralFp = `${count}`
    for (const m of messages.value) {
      structuralFp += `|${m.id}:${m.role}:${m.parts?.length ?? 0}`
    }

    // Array reference changed (loadMessages, initSession, rewind) => always rebuild
    const arrayRef = messages.value
    const arrayChanged = arrayRef !== _dispLastArray
    _dispLastArray = arrayRef

    if (!arrayChanged && structuralFp === _dispFingerprint) {
      // Same array, same structure — check for content-only changes (db_updated)
      let contentChanged = false
      for (const m of messages.value) {
        const prev = _contentLengths.get(m.id) ?? 0
        const curr = m.content?.length ?? 0
        if (curr !== prev) {
          contentChanged = true
          _contentLengths.set(m.id, curr)
        }
      }
      if (!contentChanged) return _dispCache
      // Content changed — fall through to rebuild
    } else {
      // Structural change — update content lengths
      _contentLengths.clear()
      for (const m of messages.value) {
        _contentLengths.set(m.id, m.content?.length ?? 0)
      }
    }

    _dispFingerprint = structuralFp

    // Collect tool_results from hidden messages (keyed by tool_use_id)
    const toolResults = new Map<string, string>()
    for (const m of messages.value) {
      if (m.parts) {
        for (const p of m.parts) {
          if (p.part_type === 'tool_result' && p.tool_use_id) {
            toolResults.set(p.tool_use_id, p.content)
          }
        }
      }
    }

    const result: UIMessage[] = []
    for (const m of messages.value) {
      const hidden = isInternalCacheMessage(m)

      // Assistant messages with parts: interleave text/thinking/tool_use in part_order
      if (m.role === 'assistant' && m.parts && m.parts.length > 0 && !hidden) {
        let textBuf = ''
        let thinkBuf = ''
        for (const p of m.parts) {
          if (p.part_type === 'thinking') {
            thinkBuf += p.content
          } else if (p.part_type === 'text') {
            textBuf += p.content
          } else if (p.part_type === 'tool_use') {
            // Skip todo_write — handled by TodoPanel instead of inline ToolCard
            if (p.tool_name === 'todo_write') continue
            if (textBuf.trim() || thinkBuf) {
              result.push({ ...m, id: `${m.id}-t${result.length}`, dbId: Number(m.id), thinking: thinkBuf || undefined, content: textBuf, parts: [] } as UIMessage)
              textBuf = ''; thinkBuf = ''
            }
            const tcId = p.tool_use_id || ''
            result.push({
              id: `tool-${tcId}`,
              session_id: m.session_id,
              role: 'tool' as const,
              content: toolResults.get(tcId) || '',
              parts: [],
              tool_calls: JSON.stringify([{ id: tcId, type: 'function', function: { name: p.tool_name || '', arguments: p.tool_input || '{}' } }]),
              created_at: m.created_at,
            } as UIMessage)
          }
        }
        if (textBuf.trim() || thinkBuf) {
          result.push({ ...m, id: `${m.id}-t${result.length}`, dbId: Number(m.id), thinking: thinkBuf || undefined, content: textBuf, parts: [] } as UIMessage)
        }
        continue
      }

      // Non-assistant or no parts: show as-is (unless hidden)
      if (!hidden) {
        if (m.content?.startsWith('💭')) {
          const parts = m.content.split('\n\n')
          result.push({ ...m, id: `${m.id}-t${result.length}`, dbId: Number(m.id), thinking: parts[0].replace('💭 ', ''), content: parts.slice(1).join('\n\n'), parts: m.parts || [] } as UIMessage)
        } else {
          result.push({ ...m, id: `${m.id}-t${result.length}`, dbId: Number(m.id), thinking: undefined, parts: m.parts || [] } as UIMessage)
        }
      }
    }
    // Attach duration to the last assistant message
    const sid = sessionId.value
    const dur = sid != null ? sessionDuration.get(sid) : undefined
    if (dur != null) {
      for (let i = result.length - 1; i >= 0; i--) {
        if (result[i].role === 'assistant') {
          result[i] = { ...result[i], duration_ms: dur }
          break
        }
      }
    }
    _dispCache = result
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
    clearSessionCache,
    switchGroupChat,
    tokenUsage,
    cacheUsage,
    retryMessage,
    retryMessageById,
    showRewindConfirm,
    rewindToMessage,
    confirmRewind,
    cancelRewind,
    showClearConfirm,
    clearConversation,
  }
}
