import { describe, it, expect, beforeEach, vi } from 'vitest'

const { mockListen, mockInvoke } = vi.hoisted(() => ({
  mockListen: vi.fn(),
  mockInvoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: mockListen,
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}))

import { useBackgroundTasks, type BgTask } from '../composables/useBackgroundTasks'

describe('useBackgroundTasks', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Reset singleton state between test suites
    const c = useBackgroundTasks()
    c.stopListener()
  })

  describe('initial state', () => {
    it('starts with empty tasks map', () => {
      const c = useBackgroundTasks()
      expect(c.tasks.value).toBeInstanceOf(Map)
    })
  })

  describe('toggleCollapse', () => {
    it('toggles collapsed between true/false', () => {
      const c = useBackgroundTasks()
      const m = new Map(c.tasks.value)
      m.set(1, { id: 1, pid: 100, command: 'test', out_file: '', start_time: 0,
        running: true, exit_code: null, output: '', collapsed: true })
      c.tasks.value = m

      c.toggleCollapse(1)
      expect(c.tasks.value.get(1)!.collapsed).toBe(false)
      c.toggleCollapse(1)
      expect(c.tasks.value.get(1)!.collapsed).toBe(true)
    })
  })

  describe('killTask', () => {
    it('calls invoke with correct args', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)
      const c = useBackgroundTasks()
      const m = new Map(c.tasks.value)
      m.set(1, { id: 1, pid: 100, command: 'test', out_file: '', start_time: 0,
        running: true, exit_code: null, output: '', collapsed: true })
      c.tasks.value = m

      await c.killTask(1)
      expect(mockInvoke).toHaveBeenCalledWith('kill_bg_task', { taskId: 1 })
      expect(c.tasks.value.get(1)!.running).toBe(false)
    })
  })

  describe('startListener', () => {
    it('registers a Tauri event listener', () => {
      mockListen.mockResolvedValueOnce(vi.fn())
      const c = useBackgroundTasks()
      c.startListener(1)
      expect(mockListen).toHaveBeenCalledWith('background_task_output', expect.any(Function))
    })
  })

  describe('stopListener', () => {
    it('clears tasks after stop', async () => {
      mockListen.mockResolvedValueOnce(vi.fn())
      const c = useBackgroundTasks()
      c.startListener(1)

      // Manually add a task
      const m = new Map(c.tasks.value)
      m.set(1, { id: 1, pid: 100, command: 'test', out_file: '', start_time: 0,
        running: true, exit_code: null, output: '', collapsed: true })
      c.tasks.value = m

      c.stopListener()
      // stopListener clears allTasks
      expect(c.tasks.value.size).toBe(0)
    })

    it('allows restartListener after stop', async () => {
      mockListen.mockResolvedValueOnce(vi.fn())
      const c = useBackgroundTasks()
      c.startListener(1)
      c.stopListener()

      // After stop, should be able to start again
      mockListen.mockResolvedValueOnce(vi.fn())
      c.startListener(2)
      expect(mockListen).toHaveBeenCalledTimes(2)
    })
  })

  describe('event handling', () => {
    it('creates task from started event with correct fields', () => {
      let handler: any = null
      mockListen.mockImplementationOnce((_e: string, h: any) => {
        handler = h
        return Promise.resolve(vi.fn())
      })
      const c = useBackgroundTasks()
      c.startListener(1)

      handler({ payload: {
        task_id: 10, session_id: 1, pid: 200, command: 'echo hello',
        type: 'started', output_delta: '', out_file: '/tmp/test.txt',
        start_time: 1700000000, exit_code: null,
      }})

      const t = c.tasks.value.get(10)
      expect(t).toBeDefined()
      expect(t!.command).toBe('echo hello')
      expect(t!.pid).toBe(200)
      expect(t!.out_file).toBe('/tmp/test.txt')
      expect(t!.start_time).toBe(1700000000)
      expect(t!.running).toBe(true)
    })

    it('accumulates output from output events', () => {
      let handler: any = null
      mockListen.mockImplementationOnce((_e: string, h: any) => {
        handler = h
        return Promise.resolve(vi.fn())
      })
      const c = useBackgroundTasks()
      c.startListener(1)

      handler({ payload: { task_id: 5, session_id: 1, pid: 500, command: 'x',
        type: 'started', output_delta: '', out_file: '', start_time: 0, exit_code: null }})
      handler({ payload: { task_id: 5, session_id: 1, pid: 500, command: 'x',
        type: 'output', output_delta: 'Hello World', out_file: '', start_time: 0, exit_code: null }})

      expect(c.tasks.value.get(5)!.output).toContain('Hello World')
    })

    it('marks task as done on exited event', () => {
      let handler: any = null
      mockListen.mockImplementationOnce((_e: string, h: any) => {
        handler = h
        return Promise.resolve(vi.fn())
      })
      const c = useBackgroundTasks()
      c.startListener(1)

      handler({ payload: { task_id: 3, session_id: 1, pid: 300, command: 'x',
        type: 'started', output_delta: '', out_file: '', start_time: 0, exit_code: null }})
      handler({ payload: { task_id: 3, session_id: 1, pid: 300, command: 'x',
        type: 'exited', output_delta: '', out_file: '', start_time: 0, exit_code: 0 }})

      expect(c.tasks.value.get(3)!.running).toBe(false)
      expect(c.tasks.value.get(3)!.exit_code).toBe(0)
    })

    it('filters out events from other sessions', () => {
      let handler: any = null
      mockListen.mockImplementationOnce((_e: string, h: any) => {
        handler = h
        return Promise.resolve(vi.fn())
      })
      const c = useBackgroundTasks()
      c.startListener(1)

      handler({ payload: { task_id: 99, session_id: 999, pid: 999, command: 'x',
        type: 'started', output_delta: '', out_file: '', start_time: 0, exit_code: null }})

      expect(c.tasks.value.size).toBe(0)
    })
  })

  describe('refreshTasks', () => {
    it('populates tasks from backend', async () => {
      const backend: BgTask[] = [{
        id: 42, pid: 4200, command: 'npm run dev',
        out_file: '/tmp/dev_out.txt', start_time: 1700000000,
        running: true, exit_code: null,
      }]
      mockInvoke.mockResolvedValueOnce(backend)
      const c = useBackgroundTasks()
      await c.refreshTasks(1)
      expect(c.tasks.value.get(42)!.command).toBe('npm run dev')
      expect(c.tasks.value.get(42)!.out_file).toBe('/tmp/dev_out.txt')
    })

    it('preserves existing output on refresh', async () => {
      const c = useBackgroundTasks()
      const m = new Map(c.tasks.value)
      m.set(99, { id: 99, pid: 999, command: 'x', out_file: '/t',
        start_time: 0, running: true, exit_code: null,
        output: 'old output', collapsed: true })
      c.tasks.value = m

      const backend: BgTask[] = [{ id: 99, pid: 999, command: 'x',
        out_file: '/t', start_time: 0, running: true, exit_code: null }]
      mockInvoke.mockResolvedValueOnce(backend)
      await c.refreshTasks(1)
      expect(c.tasks.value.get(99)!.output).toBe('old output')
    })
  })

  describe('loadFullOutput', () => {
    it('reads from backend and updates task output', async () => {
      mockInvoke.mockResolvedValueOnce('full content\nline 2\n')
      const c = useBackgroundTasks()
      const m = new Map(c.tasks.value)
      m.set(1, { id: 1, pid: 100, command: 'test', out_file: '/tmp/out.txt',
        start_time: 0, running: true, exit_code: null, output: '', collapsed: false })
      c.tasks.value = m

      await c.loadFullOutput(1)
      expect(mockInvoke).toHaveBeenCalledWith('read_bg_output', {
        outFile: '/tmp/out.txt', tailLines: 500,
      })
      expect(c.tasks.value.get(1)!.output).toBe('full content\nline 2\n')
    })
  })
})
