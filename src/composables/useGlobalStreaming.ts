// useGlobalStreaming — singleton stream state store.
//
// Updates are synchronous — the browser's animation-frame batching (~16ms)
// naturally coalesces rapid updates into a single paint. The caller
// (useAgentConversation) uses requestAnimationFrame to pace calls.

import { shallowRef, triggerRef } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export interface StreamEventPayload {
  type: string
  content?: string
  thinking?: string
  done?: boolean
  tool?: string
  tool_id?: string
  input?: Record<string, unknown>
  result?: string
  textBefore?: string
  toolCallCount?: number
  cache_hit_tokens?: number
  cache_miss_tokens?: number
  cache_hit_ratio?: number
  estimated_tokens?: number
  context_window?: number
  usage_pct?: number
  message_id?: number
  call_id?: string
  stream?: 'stdout' | 'stderr'
  chunk_b64?: string
  exit_code?: number | null
  killed?: boolean
  truncated?: boolean
  command?: string
  cwd?: string
  error?: string
}

export interface StreamToolEvent {
  type: 'tool_start' | 'tool_end'
  tool: string
  tool_id: string
  input?: Record<string, unknown>
  result?: string
  textBefore?: string
  thinkingBefore?: string
}

interface StreamState {
  text: string
  thinking: string
  done: boolean
  started: boolean
  abort: (() => void) | null
  toolCallCount: number
  toolEvents: StreamToolEvent[]
  lastAccess: number
}

// Module-level singleton — both display state AND Tauri listener handles.
interface SessionEntry {
  state: StreamState
  unlisten: UnlistenFn | null
}

const MAX_SESSIONS = 10
const MAX_TOOL_EVENTS = 50

const sessions = shallowRef<Map<string, SessionEntry>>(new Map())

// Track sessions that have an active frontend listener (mounted agent tab).
export const activeFrontendSessions = new Set<number>()

function getStreamKey(sessionId: number | null): string {
  return `agent_stream_${sessionId ?? 'null'}`
}

function defaultState(): StreamState {
  return { text: '', thinking: '', done: true, started: false, abort: null, toolCallCount: 0, toolEvents: [], lastAccess: Date.now() }
}

/** Evict oldest idle sessions to cap memory usage. */
function evictOldSessions() {
  const map = sessions.value
  if (map.size <= MAX_SESSIONS) return

  const entries = [...map.entries()]
    .filter(([, e]) => e.state.done)
    .sort((a, b) => (a[1].state.lastAccess || 0) - (b[1].state.lastAccess || 0))

  const toEvict = entries.slice(0, map.size - MAX_SESSIONS)
  if (toEvict.length === 0) return

  for (const [key, entry] of toEvict) {
    if (entry.unlisten) entry.unlisten()
    map.delete(key)
  }
  triggerRef(sessions)
}

export function useGlobalStreaming() {
  function getStreamState(sessionId: number | null): StreamState {
    const key = getStreamKey(sessionId)
    const entry = sessions.value.get(key)
    if (entry) {
      entry.state.lastAccess = Date.now()
      return entry.state
    }
    return defaultState()
  }

  function updateStreamState(sessionId: number | null, updates: Partial<StreamState>) {
    const key = getStreamKey(sessionId)
    const existing = sessions.value.get(key)
    const current = existing?.state ?? defaultState()
    const merged: StreamState = {
      ...current,
      ...updates,
      lastAccess: Date.now(),
    }
    if (updates.toolEvents) {
      const te = updates.toolEvents
      merged.toolEvents = te.length > MAX_TOOL_EVENTS ? te.slice(-MAX_TOOL_EVENTS) : te
    }
    if (existing) {
      existing.state = merged
    } else {
      sessions.value.set(key, { state: merged, unlisten: null })
    }
    triggerRef(sessions)
    if (!existing) evictOldSessions()
  }

  function clearStreamState(sessionId: number | null) {
    const key = getStreamKey(sessionId)
    const entry = sessions.value.get(key)
    if (entry?.state.abort) {
      entry.state.abort()
    }
    if (entry) {
      entry.state = defaultState()
    } else {
      sessions.value.set(key, { state: defaultState(), unlisten: null })
    }
    triggerRef(sessions)
  }

  async function setupListener(
    sessionId: number,
    handler: (event: { payload: StreamEventPayload }) => void,
  ): Promise<UnlistenFn> {
    const key = getStreamKey(sessionId)
    await teardownListener(sessionId)
    const unlisten = await listen<StreamEventPayload>(`agent_stream_${sessionId}`, handler)
    const existing = sessions.value.get(key)
    if (existing) {
      existing.unlisten = unlisten
    } else {
      sessions.value.set(key, { state: defaultState(), unlisten })
    }
    triggerRef(sessions)
    evictOldSessions()
    return unlisten
  }

  async function teardownListener(sessionId: number | null) {
    if (sessionId === null) return
    const key = getStreamKey(sessionId)
    const entry = sessions.value.get(key)
    if (entry?.unlisten) {
      entry.unlisten()
    }
    if (entry) {
      entry.unlisten = null
      triggerRef(sessions)
    }
  }

  function clearAgentStreams() {
    const map = sessions.value
    let changed = false
    for (const [key, entry] of map) {
      if (key.includes('agent_stream_')) {
        if (entry.state.abort) entry.state.abort()
        if (entry.unlisten) entry.unlisten()
        map.delete(key)
        changed = true
      }
    }
    if (changed) triggerRef(sessions)
  }

  return {
    sessions,
    getStreamState,
    updateStreamState,
    clearStreamState,
    setupListener,
    teardownListener,
    clearAgentStreams,
  }
}
