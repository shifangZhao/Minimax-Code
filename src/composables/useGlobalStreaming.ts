import { shallowRef } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

interface StreamState {
  text: string
  thinking: string
  done: boolean
  abort: (() => void) | null
  toolCallCount: number
}

// Module-level singleton — both display state AND Tauri listener handles.
// Per-session key: `agent_stream_${sessionId}`
interface SessionEntry {
  state: StreamState
  unlisten: UnlistenFn | null
}

const sessions = shallowRef<Map<string, SessionEntry>>(new Map())

function getStreamKey(sessionId: number | null): string {
  return `agent_stream_${sessionId ?? 'null'}`
}

export function useGlobalStreaming() {
  function getStreamState(sessionId: number | null): StreamState {
    const key = getStreamKey(sessionId)
    return sessions.value.get(key)?.state
      || { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 }
  }

  function updateStreamState(sessionId: number | null, updates: Partial<StreamState>) {
    const key = getStreamKey(sessionId)
    const newMap = new Map(sessions.value)
    const entry = newMap.get(key)
    if (entry) {
      newMap.set(key, { ...entry, state: { ...entry.state, ...updates } })
    } else {
      newMap.set(key, {
        state: { text: '', thinking: '', done: true, abort: null, toolCallCount: 0, ...updates },
        unlisten: null,
      })
    }
    sessions.value = newMap
  }

  function clearStreamState(sessionId: number | null) {
    const key = getStreamKey(sessionId)
    const entry = sessions.value.get(key)
    if (entry?.state.abort) {
      entry.state.abort()
    }
    // Keep the unlisten handle alive — the caller manages listener lifecycle
    const newMap = new Map(sessions.value)
    newMap.set(key, {
      state: { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 },
      unlisten: entry?.unlisten ?? null,
    })
    sessions.value = newMap
  }

  // Full lifecycle: register a Tauri event listener and tie it to the session.
  // Returns the unlisten function (also stored internally for cleanup).
  async function setupListener(
    sessionId: number,
    handler: (event: any) => void,
  ): Promise<UnlistenFn> {
    const key = getStreamKey(sessionId)
    // Tear down any previous listener for this session
    await teardownListener(sessionId)
    const unlisten = await listen<any>(`agent_stream_${sessionId}`, handler)
    const newMap = new Map(sessions.value)
    const existing = newMap.get(key)
    newMap.set(key, {
      state: existing?.state ?? { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 },
      unlisten,
    })
    sessions.value = newMap
    return unlisten
  }

  async function teardownListener(sessionId: number | null) {
    if (sessionId === null) return
    const key = getStreamKey(sessionId)
    const entry = sessions.value.get(key)
    if (entry?.unlisten) {
      entry.unlisten()
    }
    const newMap = new Map(sessions.value)
    newMap.set(key, {
      state: entry?.state ?? { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 },
      unlisten: null,
    })
    sessions.value = newMap
  }

  function clearAgentStreams() {
    const newMap = new Map(sessions.value)
    for (const key of newMap.keys()) {
      if (key.includes('agent_stream_')) {
        const entry = newMap.get(key)
        if (entry?.state.abort) entry.state.abort()
        if (entry?.unlisten) entry.unlisten()
        newMap.set(key, {
          state: { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 },
          unlisten: null,
        })
      }
    }
    sessions.value = newMap
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
