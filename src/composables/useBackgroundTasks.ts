import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

export interface BgTask {
  id: number
  pid: number
  command: string
  out_file: string
  start_time: number
  running: boolean
  exit_code: number | null
}

export interface BgTaskWithOutput extends BgTask {
  output: string
  collapsed: boolean
}

interface TaskOutputEvent {
  task_id: number
  session_id: number
  pid: number
  command: string
  type: 'started' | 'output' | 'exited'
  output_delta: string
  out_file: string
  start_time: number
  exit_code: number | null
}

// Module-level singleton: one global Tauri listener, tasks from all sessions.
const allTasks = ref<Map<number, BgTaskWithOutput>>(new Map())
let unlisten: UnlistenFn | null = null
let listenerStarted = false
let activeSessionId: number | null = null

export function useBackgroundTasks() {
  function startListener(sessionId?: number) {
    if (sessionId != null) activeSessionId = sessionId
    if (listenerStarted) return
    listenerStarted = true
    listen<TaskOutputEvent>('background_task_output', (evt) => {
      const e = evt.payload
      if (activeSessionId != null && e.session_id !== activeSessionId) return
      const m = new Map(allTasks.value)
      const existing = m.get(e.task_id)
      if (existing) {
        existing.output += e.output_delta
        if (e.type === 'exited') {
          existing.running = false
          existing.exit_code = e.exit_code
          // Auto-remove completed tasks after 3 seconds
          setTimeout(() => {
            const m2 = new Map(allTasks.value)
            m2.delete(e.task_id)
            allTasks.value = m2
            // Also remove from backend
            invoke('remove_bg_task', { taskId: e.task_id }).catch(() => {})
          }, 3000)
        }
      } else {
        m.set(e.task_id, {
          id: e.task_id,
          pid: e.pid,
          command: e.command,
          out_file: e.out_file || '',
          start_time: e.start_time || 0,
          running: e.type !== 'exited',
          exit_code: e.exit_code,
          output: e.output_delta,
          collapsed: true,
        })
      }
      allTasks.value = m
    }).then(fn => { unlisten = fn })
  }

  function stopListener() {
    unlisten?.()
    unlisten = null
    listenerStarted = false
    activeSessionId = null
    allTasks.value = new Map()
  }

  async function refreshTasks(sessionId: number) {
    activeSessionId = sessionId
    try {
      const list: BgTask[] = await invoke('list_bg_tasks', { sessionId })
      const m = new Map<number, BgTaskWithOutput>()
      for (const t of list) {
        const old = allTasks.value.get(t.id)
        m.set(t.id, {
          ...t,
          output: old?.output ?? '',
          collapsed: old?.collapsed ?? true,
        })
      }
      allTasks.value = m
    } catch (_) { /* ignore */ }
  }

  async function killTask(taskId: number) {
    try {
      await invoke('kill_bg_task', { taskId })
      const m = new Map(allTasks.value)
      m.delete(taskId)
      allTasks.value = m
    } catch (_) { /* ignore */ }
  }

  async function removeTask(taskId: number) {
    // Sync removal with backend registry so the task doesn't keep showing up
    // on next list_bg_tasks refresh.
    try { await invoke('remove_bg_task', { taskId }) } catch (_) { /* ignore */ }
    const m = new Map(allTasks.value)
    m.delete(taskId)
    allTasks.value = m
  }

  async function loadFullOutput(taskId: number) {
    try {
      const t = allTasks.value.get(taskId)
      if (!t) return
      const full: string = await invoke('read_bg_output', { outFile: t.out_file, tailLines: 500 })
      const m = new Map(allTasks.value)
      const entry = m.get(taskId)
      if (entry) { entry.output = full; m.set(taskId, entry) }
      allTasks.value = m
    } catch (_) { /* ignore */ }
  }

  function toggleCollapse(taskId: number) {
    const m = new Map(allTasks.value)
    const t = m.get(taskId)
    if (t) { t.collapsed = !t.collapsed; m.set(taskId, t) }
    allTasks.value = m
  }

  return {
    tasks: allTasks,
    startListener,
    stopListener,
    refreshTasks,
    killTask,
    removeTask,
    loadFullOutput,
    toggleCollapse,
  }
}
