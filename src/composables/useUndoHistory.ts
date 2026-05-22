import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface FileSnapshot {
  id: number
  session_id: number
  file_path: string
  original_content?: string
  created_at: string
}

export function useUndoHistory() {
  const recentEdits = ref<FileSnapshot[]>([])
  const lastUndone = ref<string | null>(null)
  const showUndoToast = ref(false)

  async function loadEdits(sessionId: number) {
    try {
      recentEdits.value = await invoke<FileSnapshot[]>('list_edits', { sessionId })
    } catch { recentEdits.value = [] }
  }

  async function undoLast(sessionId: number): Promise<boolean> {
    try {
      const snap = await invoke<FileSnapshot>('undo_last_edit', { sessionId })
      lastUndone.value = snap.file_path
      showUndoToast.value = true
      setTimeout(() => { showUndoToast.value = false }, 4000)
      await loadEdits(sessionId)
      return true
    } catch { return false }
  }

  async function undoById(snapshotId: number, sessionId: number): Promise<boolean> {
    try {
      await invoke('undo_edit_by_id', { snapshotId })
      await loadEdits(sessionId)
      return true
    } catch { return false }
  }

  return { recentEdits, lastUndone, showUndoToast, loadEdits, undoLast, undoById }
}
