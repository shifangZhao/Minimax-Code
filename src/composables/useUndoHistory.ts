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
  const lastUndoError = ref<string | null>(null)

  async function loadEdits(sessionId: number) {
    try {
      recentEdits.value = await invoke<FileSnapshot[]>('list_edits', { sessionId })
    } catch (e) {
      console.error('[useUndoHistory] loadEdits failed:', e)
      recentEdits.value = []
    }
  }

  async function undoLast(sessionId: number): Promise<boolean> {
    try {
      const snap = await invoke<FileSnapshot>('undo_last_edit', { sessionId })
      lastUndone.value = snap.file_path
      lastUndoError.value = null
      showUndoToast.value = true
      setTimeout(() => { showUndoToast.value = false }, 4000)
      await loadEdits(sessionId)
      return true
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      console.error('[useUndoHistory] undoLast failed:', msg, 'sessionId=', sessionId)
      lastUndoError.value = msg
      return false
    }
  }

  async function undoById(snapshotId: number, sessionId: number): Promise<boolean> {
    try {
      await invoke('undo_edit_by_id', { snapshotId })
      await loadEdits(sessionId)
      return true
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      console.error('[useUndoHistory] undoById failed:', msg, 'snapshotId=', snapshotId)
      lastUndoError.value = msg
      return false
    }
  }

  return { recentEdits, lastUndone, showUndoToast, lastUndoError, loadEdits, undoLast, undoById }
}
