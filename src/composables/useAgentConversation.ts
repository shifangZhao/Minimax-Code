// Agent conversation using Rust backend via Tauri invoke
// Supports Interleaved Thinking with complete message history

import { ref, onMounted, onUnmounted } from 'vue'
import { db, type ChatMessage } from '../services/db'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useGlobalStreaming } from './useGlobalStreaming'

export interface ToolEvent {
  type: 'tool_start' | 'tool_end'
  tool: string
  tool_id: string
  input?: Record<string, any>
  result?: string
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

interface RustStreamEvent {
  type: 'content_block_delta' | 'tool_start' | 'tool_end' | 'done' | 'error' | 'cache_usage'
  content?: string
  thinking?: string
  tool?: string
  tool_id?: string
  input?: Record<string, any>
  result?: string
  cache_hit_tokens?: number
  cache_miss_tokens?: number
  cache_hit_ratio?: number
}

export function useAgentConversation(agentType: string) {
  const messages = ref<ChatMessage[]>([])
  const loading = ref(false)  // true when current session has active stream
  const sessionId = ref<number | null>(null)
  const currentGroupChatId = ref<number | null>(null)
  const pendingAsk = ref<any>(null)
  const toolEvents = ref<ToolEvent[]>([])

  // Per-session stream listeners for multi-group-chat support
  const streamListeners = new Map<number, UnlistenFn>()
  // Per-session loading state
  const loadingSessions = new Set<number>()

  let agentInvokedUnlisten: UnlistenFn | null = null
  let askUnlisten: UnlistenFn | null = null

  // Listen for ask_choice events (only for our session, ensuring agent+group-chat isolation)
  async function setupAskListener() {
    askUnlisten = await listen<any>('ask_choice', async (event) => {
      const { id, session_id, questions } = event.payload
      // Only show if this ask is for our session
      if (session_id !== sessionId.value) return
      pendingAsk.value = { id, questions }
    })
  }

  // Listen for being invoked by other agents via send_to_agent
  async function setupAgentInvokedListener() {
    agentInvokedUnlisten = await listen<any>('agent_invoked', async (event) => {
      const { target_agent, session_id, group_chat_id } = event.payload
      if (target_agent !== agentType) return

      // Skip if already on this session (prevents duplicate DB load)
      if (sessionId.value === session_id) return

      console.log('[agent_invoked]', agentType, 'invoked, switching to session:', session_id, 'group:', group_chat_id)

      currentGroupChatId.value = group_chat_id
      sessionId.value = session_id

      await loadMessages()
    })
  }

  onMounted(() => {
    setupAgentInvokedListener()
    setupAskListener()
  })
  onUnmounted(() => {
    for (const [_, ul] of streamListeners) {
      ul()
    }
    streamListeners.clear()
    if (agentInvokedUnlisten) {
      agentInvokedUnlisten()
    }
    if (askUnlisten) {
      askUnlisten()
    }
  })

  async function initSession(groupChatId: number) {
    if (groupChatId < 0) {
      currentGroupChatId.value = groupChatId
      sessionId.value = null
      messages.value = []
      return
    }

    currentGroupChatId.value = groupChatId

    const sessions = await db.getAgentSessions(groupChatId, agentType)
    const session = sessions.find(s => s.agent_type === agentType)

    if (session) {
      sessionId.value = session.id
    } else {
      sessionId.value = await db.createAgentSession(groupChatId, agentType)
    }

    await loadMessages()
  }

  async function loadMessages() {
    if (!sessionId.value) return
    const msgs = await db.getMessages(sessionId.value)
    messages.value = msgs
  }

  // 构建发送给后端的历史消息（符合 MiniMax API 格式）
  function buildHistoryMessages(): any[] {
    const history: any[] = []

    for (const msg of messages.value) {
      if (msg.role === 'user') {
        history.push({
          role: 'user',
          content: msg.content,
          raw_json: (msg as any).raw_json || undefined
        })
      } else if (msg.role === 'assistant') {
        history.push({
          role: 'assistant',
          content: msg.content || '',
          tool_calls: msg.tool_calls || undefined,
          thinking: msg.thinking || undefined,
          raw_json: (msg as any).raw_json || undefined
        })
      } else if ((msg as any).role === 'tool' || (msg as any).raw_json) {
        // Tool result messages (stored as user role with tool_result content blocks)
        history.push({
          role: 'user',
          content: msg.content,
          raw_json: (msg as any).raw_json || undefined
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

    // Set up event listener for real-time streaming
    const { updateStreamState, clearStreamState } = useGlobalStreaming()

    // Clear and prepare stream state
    clearStreamState(finalSessionId)
    let fullText = ''
    let fullThinking = ''
    let toolCallCount = 0
    let isDone = false
    let hasError = false

    // 收集 tool_calls 信息
    const collectedToolCalls: Array<{
      id: string
      type: 'function'
      function: { name: string; arguments: string }
    }> = []

    // Clean up any existing listener for this session
    if (streamListeners.has(finalSessionId)) {
      streamListeners.get(finalSessionId)!()
      streamListeners.delete(finalSessionId)
    }

    // Set up event listener for real-time streaming
    console.log('[sendMessage] Setting up event listener for:', `agent_stream_${finalSessionId}`)
    const unlisten = await listen<RustStreamEvent>(`agent_stream_${finalSessionId}`, (event) => {
      console.log('[sendMessage] Received event:', event.payload.type)
      const ev = event.payload
      switch (ev.type) {
        case 'content_block_delta':
          if (ev.content) {
            fullText += ev.content
            updateStreamState(finalSessionId, {
              text: fullText,
              thinking: fullThinking,
              done: false,
              toolCallCount
            })
          }
          if (ev.thinking) {
            fullThinking += ev.thinking
            updateStreamState(finalSessionId, {
              text: fullText,
              thinking: fullThinking,
              done: false,
              toolCallCount
            })
          }
          break
        case 'tool_start':
          toolCallCount++
          // 收集 tool_call 信息
          if (ev.tool_id && ev.tool) {
            collectedToolCalls.push({
              id: ev.tool_id,
              type: 'function',
              function: {
                name: ev.tool,
                arguments: JSON.stringify(ev.input || {})
              }
            })
          }
          toolEvents.value.push({
            type: 'tool_start',
            tool: ev.tool || '',
            tool_id: ev.tool_id || '',
            input: ev.input
          })
          updateStreamState(finalSessionId, {
            text: fullText,
            thinking: fullThinking,
            done: false,
            toolCallCount
          })
          break
        case 'tool_end':
          toolEvents.value.push({
            type: 'tool_end',
            tool: ev.tool || '',
            tool_id: ev.tool_id || '',
            result: ev.result
          })
          break
        case 'done':
          isDone = true
          break
        case 'cache_usage':
          console.log(
            `[cache] session=${finalSessionId} hit=${ev.cache_hit_tokens} miss=${ev.cache_miss_tokens} ratio=${((ev.cache_hit_ratio || 0) * 100).toFixed(1)}%`
          )
          break
        case 'error':
          hasError = true
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

    // Store the listener for this session (multi-group-chat support)
    streamListeners.set(finalSessionId, unlisten)

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

      // Wait for stream to complete
      console.log('[sendMessage] Waiting for stream...')
      const startTime = Date.now()
      while (!isDone && !hasError && Date.now() - startTime < 120000) {
        await new Promise(resolve => setTimeout(resolve, 100))
      }

      // Final state update
      const displayContent = fullThinking
        ? `💭 ${fullThinking}\n\n${fullText}`
        : fullText
      updateStreamState(finalSessionId, {
        text: displayContent,
        thinking: fullThinking,
        done: true,
        toolCallCount
      })

      // 构建完整的 assistant 消息（用于保存到历史）
      const assistantMsg: any = {
        id: Date.now() + 1,
        session_id: finalSessionId,
        role: 'assistant',
        content: fullText,
        created_at: new Date().toISOString(),
      }

      // 如果有 reasoning_details，保存到消息中
      if (fullThinking) {
        assistantMsg.reasoning_details = fullThinking
      }

      // 如果有 tool_calls，保存到消息中
      if (collectedToolCalls.length > 0) {
        assistantMsg.tool_calls = collectedToolCalls
      }

      // Backend now persists assistant messages (with raw_json for cache).
      // Reload from DB to pick up backend-saved messages, then clear stream state.
      await loadMessages()
      updateStreamState(finalSessionId, {
        text: '',
        thinking: '',
        done: true,
        toolCallCount
      })

      toolEvents.value = []

    } catch (e: any) {
      console.error('Agent error:', e)
      hasError = true
      const errorMsg = `Error: ${e.toString()}`
      updateStreamState(finalSessionId, {
        text: errorMsg,
        done: true,
        toolCallCount
      })
      await db.addMessage(finalSessionId, 'assistant', errorMsg)
      await loadMessages()
    } finally {
      loadingSessions.delete(finalSessionId)
      loading.value = loadingSessions.size > 0
      // Clean up this session's stream listener
      const ul = streamListeners.get(finalSessionId)
      if (ul) {
        ul()
        streamListeners.delete(finalSessionId)
      }
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
    const att = (msg as any).attachments
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
      const { clearStreamState } = useGlobalStreaming()
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
      const { clearStreamState } = useGlobalStreaming()
      clearStreamState(sessionId.value)
      await db.clearSessionHistory(sessionId.value)
      messages.value = []
      toolEvents.value = []
      showClearConfirm.value = false
    } catch (e) {
      console.error('Clear failed:', e)
    }
  }

  return {
    messages,
    loading,
    sessionId,
    currentGroupChatId,
    pendingAsk,
    toolEvents,
    initSession,
    loadMessages,
    sendMessage,
    clearAsk,
    clearToolEvents,
    switchGroupChat,
    retryMessage,
    showRewindConfirm,
    rewindToMessage,
    confirmRewind,
    cancelRewind,
    showClearConfirm,
    clearConversation,
  }
}
