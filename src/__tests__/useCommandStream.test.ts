import { describe, it, expect, beforeEach, vi } from 'vitest'

const { mockInvoke } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

import {
  processCommandEvent,
  getCommandStream,
  abortCommand,
  markOrphan,
} from '../composables/useCommandStream'

describe('useCommandStream', () => {
  beforeEach(() => {
    mockInvoke.mockReset()
    mockInvoke.mockResolvedValue(undefined)
  })

  describe('processCommandEvent', () => {
    it('creates a new stream on begin', () => {
      processCommandEvent({
        type: 'exec_command_begin',
        call_id: 'c1',
        tool_id: 't1',
        command: 'npm install',
        cwd: '/proj',
      })
      const s = getCommandStream('t1')?.value
      expect(s).toBeDefined()
      expect(s?.call_id).toBe('c1')
      expect(s?.command).toBe('npm install')
      expect(s?.cwd).toBe('/proj')
      expect(s?.state).toBe('streaming')
      expect(s?.output).toBe('')
    })

    it('appends base64-decoded delta chunks to output', () => {
      processCommandEvent({ type: 'exec_command_begin', call_id: 'c1', tool_id: 't2', command: 'echo' })
      // "hello" base64 = "aGVsbG8="
      processCommandEvent({ type: 'exec_command_output_delta', call_id: 'c1', tool_id: 't2', stream: 'stdout', chunk_b64: 'aGVsbG8=' })
      // " world" base64 = "IHdvcmxk"
      processCommandEvent({ type: 'exec_command_output_delta', call_id: 'c1', tool_id: 't2', stream: 'stdout', chunk_b64: 'IHdvcmxk' })
      const s = getCommandStream('t2')?.value
      expect(s?.output).toBe('hello world')
      expect(s?.stdout).toBe('hello world')
      expect(s?.stderr).toBe('')
    })

    it('separates stdout and stderr streams', () => {
      processCommandEvent({ type: 'exec_command_begin', call_id: 'c1', tool_id: 't3', command: 'cmd' })
      processCommandEvent({ type: 'exec_command_output_delta', call_id: 'c1', tool_id: 't3', stream: 'stdout', chunk_b64: b64('ok') })
      processCommandEvent({ type: 'exec_command_output_delta', call_id: 'c1', tool_id: 't3', stream: 'stderr', chunk_b64: b64('warn') })
      const s = getCommandStream('t3')?.value
      expect(s?.stdout).toBe('ok')
      expect(s?.stderr).toBe('warn')
      expect(s?.output).toBe('okwarn') // currently concatenated; UI may want to render separately
    })

    it('marks stream as ended with exit code on end event', () => {
      processCommandEvent({ type: 'exec_command_begin', call_id: 'c1', tool_id: 't4', command: 'x' })
      processCommandEvent({ type: 'exec_command_end', call_id: 'c1', tool_id: 't4', exit_code: 0, killed: false, truncated: false })
      const s = getCommandStream('t4')?.value
      expect(s?.state).toBe('end')
      expect(s?.exit_code).toBe(0)
      expect(s?.killed).toBe(false)
    })

    it('marks stream as killed when killed=true', () => {
      processCommandEvent({ type: 'exec_command_begin', call_id: 'c1', tool_id: 't5', command: 'x' })
      processCommandEvent({ type: 'exec_command_end', call_id: 'c1', tool_id: 't5', exit_code: null, killed: true, truncated: false })
      const s = getCommandStream('t5')?.value
      expect(s?.killed).toBe(true)
    })

    it('marks stream as error on error event', () => {
      processCommandEvent({ type: 'exec_command_begin', call_id: 'c1', tool_id: 't6', command: 'x' })
      processCommandEvent({ type: 'exec_command_error', call_id: 'c1', tool_id: 't6', error: 'spawn failed' })
      const s = getCommandStream('t6')?.value
      expect(s?.state).toBe('error')
      expect(s?.error).toBe('spawn failed')
    })

    it('ignores events without tool_id', () => {
      const result = processCommandEvent({ type: 'exec_command_begin', call_id: 'c1' })
      expect(result).toBe(false)
    })

    it('ignores non-command events', () => {
      const result = processCommandEvent({ type: 'tool_start', tool: 'run_command', tool_id: 't' })
      expect(result).toBe(false)
    })
  })

  describe('markOrphan', () => {
    it('marks an in-flight stream as ended+orphan', () => {
      processCommandEvent({ type: 'exec_command_begin', call_id: 'c1', tool_id: 't7', command: 'x' })
      markOrphan('t7')
      const s = getCommandStream('t7')?.value
      expect(s?.state).toBe('end')
      expect(s?.orphan).toBe(true)
    })

    it('does not downgrade an already-ended stream', () => {
      processCommandEvent({ type: 'exec_command_begin', call_id: 'c1', tool_id: 't8', command: 'x' })
      processCommandEvent({ type: 'exec_command_end', call_id: 'c1', tool_id: 't8', exit_code: 0, killed: false, truncated: false })
      markOrphan('t8')
      const s = getCommandStream('t8')?.value
      expect(s?.state).toBe('end')
      expect(s?.orphan).toBe(false) // ended cleanly, not marked orphan
    })

    it('is a no-op for unknown tool_id', () => {
      expect(() => markOrphan('nope')).not.toThrow()
    })
  })

  describe('abortCommand', () => {
    it('invokes abort_command with the call_id', async () => {
      await abortCommand('call-xyz')
      expect(mockInvoke).toHaveBeenCalledWith('abort_command', { callId: 'call-xyz' })
    })

    it('swallows invoke errors', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('boom'))
      await expect(abortCommand('call-xyz')).resolves.toBeUndefined()
    })
  })

  describe('getCommandStream', () => {
    it('returns null for unknown tool_id', () => {
      expect(getCommandStream('nope')).toBeNull()
    })

    it('returns a reactive computed ref', () => {
      processCommandEvent({ type: 'exec_command_begin', call_id: 'c1', tool_id: 't9', command: 'x' })
      const ref = getCommandStream('t9')
      expect(ref).not.toBeNull()
      // Re-reading should give the same value (cached)
      expect(getCommandStream('t9')?.value.tool_id).toBe('t9')
    })
  })
})

function b64(s: string): string {
  // Browser-safe base64 encoding (avoids Buffer dependency in vue-tsc check)
  return btoa(unescape(encodeURIComponent(s)))
}
