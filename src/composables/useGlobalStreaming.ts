import { shallowRef } from 'vue'

interface StreamState {
  text: string
  thinking: string
  done: boolean
  abort: (() => void) | null
  toolCallCount: number
}

// Module-level singleton state - shared across all components
// Key format: `agent_stream_${sessionId}` to match Rust emit events
const globalStreamingStates = shallowRef<Map<string, StreamState>>(new Map())

// Updates are synchronous — the browser's animation-frame batching (~16ms)
// naturally coalesces rapid updates into a single paint. An extra setTimeout
// buffer would only add delay without improving throughput.

function getStreamKey(sessionId: number | null): string {
  return `agent_stream_${sessionId ?? 'null'}`
}

export function useGlobalStreaming() {
  function getStreamState(sessionId: number | null): StreamState {
    const key = getStreamKey(sessionId)
    return globalStreamingStates.value.get(key) || { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 }
  }

  function updateStreamState(sessionId: number | null, updates: Partial<StreamState>) {
    const key = getStreamKey(sessionId)
    const newMap = new Map(globalStreamingStates.value)
    const current = newMap.get(key) || { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 }
    newMap.set(key, { ...current, ...updates })
    globalStreamingStates.value = newMap
  }

  function clearStreamState(sessionId: number | null) {
    const key = getStreamKey(sessionId)
    const state = globalStreamingStates.value.get(key)
    if (state?.abort) {
      state.abort()
    }
    const newMap = new Map(globalStreamingStates.value)
    newMap.set(key, { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 })
    globalStreamingStates.value = newMap
  }

  // Clear all streams
  function clearAgentStreams() {
    const newMap = new Map(globalStreamingStates.value)
    for (const key of newMap.keys()) {
      if (key.includes('agent_stream_')) {
        const state = newMap.get(key)
        if (state?.abort) {
          state.abort()
        }
        newMap.set(key, { text: '', thinking: '', done: true, abort: null, toolCallCount: 0 })
      }
    }
    globalStreamingStates.value = newMap
  }

  return {
    globalStreamingStates,
    getStreamState,
    updateStreamState,
    clearStreamState,
    clearAgentStreams,
  }
}
