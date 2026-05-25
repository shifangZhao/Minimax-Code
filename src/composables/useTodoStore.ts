import { shallowRef } from 'vue'

export interface TodoItem {
  content: string
  status: 'pending' | 'in_progress' | 'completed'
  activeForm?: string
}

export interface TodoState {
  items: TodoItem[]
  summary: string
  pct: number
}

// Module-level singleton — per-session todo state.
// Updated from tool_end events during streaming, restored from message history on reload.
const todoStates = shallowRef<Map<number, TodoState>>(new Map())

export function useTodoStore() {
  function getState(sessionId: number | null): TodoState | null {
    if (sessionId === null) return null
    return todoStates.value.get(sessionId) ?? null
  }

  /** Parse a todo_write tool_end result and update state. */
  function updateFromResult(sessionId: number, resultJson: string) {
    try {
      const parsed = JSON.parse(resultJson)
      if (parsed.todos && Array.isArray(parsed.todos)) {
        const items: TodoItem[] = parsed.todos.map((t: any) => ({
          content: t.content || '',
          status: t.status || 'pending',
          activeForm: t.activeForm,
        }))
        const state: TodoState = {
          items,
          summary: parsed.summary || '',
          pct: parsed.pct ?? 0,
        }
        const newMap = new Map(todoStates.value)
        newMap.set(sessionId, state)
        todoStates.value = newMap
      }
    } catch {
      // ignore parse errors — result might not be valid JSON
    }
  }

  /** Scan persisted messages for the last todo_write tool result. */
  function restoreFromMessages(sessionId: number, parts: Array<{ part_type: string; tool_name?: string; content: string }>) {
    // Walk parts in reverse to find the last todo_write result
    for (let i = parts.length - 1; i >= 0; i--) {
      const p = parts[i]
      if (p.part_type === 'tool_result' && p.tool_name === 'todo_write' && p.content) {
        updateFromResult(sessionId, p.content)
        return
      }
    }
    // No todo_write found — leave state as-is (may be stale from streaming, cleared on new session)
  }

  function clearState(sessionId: number) {
    const newMap = new Map(todoStates.value)
    newMap.delete(sessionId)
    todoStates.value = newMap
  }

  function clearAll() {
    todoStates.value = new Map()
  }

  return {
    todoStates,
    getState,
    updateFromResult,
    restoreFromMessages,
    clearState,
    clearAll,
  }
}
