// useCommandStream — subscribes to the streaming-command protocol
// (exec_command_begin/delta/end/error) emitted over the existing
// `agent_stream_${sessionId}` Tauri channel, and exposes a per-tool
// reactive buffer of accumulated output.
//
// Performance optimizations:
// 1. Periodic cleanup every 60s for ended/orphaned streams
// 2. Output capped at 512KB (from 1MB) to reduce memory
// 3. Max 30 states kept (from 50)
// 4. Ended streams cleaned after 2min (from 5min)

import { computed, type ComputedRef, type Ref, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { StreamEventPayload } from './useGlobalStreaming'

export type CommandState = 'begin' | 'streaming' | 'end' | 'error'

export interface CommandStreamState {
  call_id: string
  tool_id: string
  command: string
  cwd?: string
  output: string
  stdout: string
  stderr: string
  state: CommandState
  exit_code: number | null
  killed: boolean
  truncated: boolean
  error?: string
  orphan: boolean
  _lastTouched?: number
}

// tool_id -> reactive state
const states = new Map<string, Ref<CommandStreamState>>()

const ENDED_TTL_MS = 2 * 60 * 1000   // 2 min (was 5)
const MAX_OUTPUT_SIZE = 512 * 1024    // 512KB (was 1MB)
const MAX_STATES = 30                 // (was 50)

function limitOutput(output: string): string {
  if (output.length <= MAX_OUTPUT_SIZE) return output
  const keep = MAX_OUTPUT_SIZE - 200
  return output.slice(0, keep / 2) + '\n... [truncated] ...\n' + output.slice(-keep / 2)
}

/** Remove completed/orphaned states exceeding limits. */
function prune() {
  const now = Date.now()
  // 1. Remove expired orphans and ended
  for (const [id, r] of states) {
    const s = r.value
    if ((s.state === 'end' || s.state === 'error') && (s._lastTouched ?? 0) + ENDED_TTL_MS < now) {
      states.delete(id)
    }
  }
  // 2. Hard cap: remove oldest ended entries
  if (states.size > MAX_STATES) {
    const ended = [...states.entries()]
      .filter(([, r]) => r.value.state === 'end' || r.value.state === 'error')
      .sort((a, b) => (a[1].value._lastTouched ?? 0) - (b[1].value._lastTouched ?? 0))
    for (let i = 0; states.size > MAX_STATES && i < ended.length; i++) {
      states.delete(ended[i][0])
    }
  }
}

// Periodic sweep: every 60s
let sweepTimer: ReturnType<typeof setInterval> | null = null
function ensureSweep() {
  if (sweepTimer) return
  sweepTimer = setInterval(prune, 60_000)
}

function getOrCreate(toolId: string, init: Partial<CommandStreamState>): Ref<CommandStreamState> {
  let r = states.get(toolId)
  if (!r) {
    r = ref<CommandStreamState>({
      call_id: '',
      tool_id: toolId,
      command: '',
      output: '',
      stdout: '',
      stderr: '',
      state: 'begin',
      exit_code: null,
      killed: false,
      truncated: false,
      orphan: false,
      _lastTouched: Date.now(),
      ...init,
    })
    states.set(toolId, r)
    ensureSweep()
  }
  return r
}

function decodeChunk(b64: string): string {
  if (!b64) return ''
  try { return atob(b64) } catch { return '' }
}

export function processCommandEvent(ev: StreamEventPayload): boolean {
  if (!ev.type?.startsWith('exec_command_')) return false
  if (!ev.tool_id) return false

  const r = getOrCreate(ev.tool_id, { call_id: ev.call_id ?? '', tool_id: ev.tool_id })

  switch (ev.type) {
    case 'exec_command_begin': {
      r.value = {
        ...r.value,
        call_id: ev.call_id ?? r.value.call_id,
        command: ev.command ?? '',
        cwd: ev.cwd,
        output: '',
        stdout: '',
        stderr: '',
        state: 'streaming',
        exit_code: null,
        killed: false,
        truncated: false,
        orphan: false,
        _lastTouched: Date.now(),
      }
      break
    }
    case 'exec_command_output_delta': {
      const text = decodeChunk(ev.chunk_b64 ?? '')
      const stream = ev.stream ?? 'stdout'
      const nextStdout = stream === 'stderr' ? r.value.stdout : r.value.stdout + text
      const nextStderr = stream === 'stderr' ? r.value.stderr + text : r.value.stderr
      r.value = {
        ...r.value,
        stdout: limitOutput(nextStdout),
        stderr: limitOutput(nextStderr),
        output: limitOutput(nextStdout + nextStderr),
        state: 'streaming',
        _lastTouched: Date.now(),
      }
      break
    }
    case 'exec_command_end': {
      r.value = {
        ...r.value,
        state: 'end',
        exit_code: ev.exit_code ?? null,
        killed: !!ev.killed,
        truncated: !!ev.truncated,
        orphan: false,
        _lastTouched: Date.now(),
      }
      break
    }
    case 'exec_command_error': {
      r.value = {
        ...r.value,
        state: 'error',
        error: ev.error ?? 'unknown error',
        orphan: false,
        _lastTouched: Date.now(),
      }
      break
    }
  }
  return true
}

export function markOrphan(toolId: string) {
  const r = states.get(toolId)
  if (!r) return
  if (r.value.state === 'begin' || r.value.state === 'streaming') {
    r.value = { ...r.value, state: 'end', orphan: true, _lastTouched: Date.now() }
  }
}

export function getCommandStream(toolId: string): ComputedRef<CommandStreamState> | null {
  const r = states.get(toolId)
  if (!r) return null
  return computed(() => r.value)
}

export function getAllCommandStreams(): ComputedRef<CommandStreamState>[] {
  return [...states.values()].map((r) => computed(() => r.value))
}

export async function abortCommand(callId: string): Promise<void> {
  try { await invoke('abort_command', { callId }) }
  catch (e) { console.error('[useCommandStream] abort_command failed:', e) }
}

export function useCommandStream() {
  return { processCommandEvent, markOrphan, getCommandStream, getAllCommandStreams, abortCommand }
}
