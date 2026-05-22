import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface ConversationBookmark {
  id: number
  session_id: number
  name: string
  file_snapshots: string
  message_count: number
  total_bytes: number
  created_at: string
}

export function useBookmarks() {
  const bookmarks = ref<ConversationBookmark[]>([])
  const showBookmarkPanel = ref(false)
  const showRestoreConfirm = ref<ConversationBookmark | null>(null)
  const showSaveInput = ref(false)
  const bookmarkName = ref('')

  async function loadBookmarks(sessionId: number) {
    try {
      bookmarks.value = await invoke<ConversationBookmark[]>('list_bookmarks', { sessionId })
    } catch { bookmarks.value = [] }
  }

  async function saveBookmark(sessionId: number, workspace: string): Promise<boolean> {
    try {
      const name = bookmarkName.value.trim() || new Date().toLocaleString()
      await invoke('save_bookmark', { sessionId, name, workspace })
      bookmarkName.value = ''
      showSaveInput.value = false
      await loadBookmarks(sessionId)
      return true
    } catch (e) {
      console.error('Failed to save bookmark:', e)
      return false
    }
  }

  async function restoreBookmark(bookmarkId: number, workspace: string): Promise<boolean> {
    try {
      await invoke('restore_bookmark', { bookmarkId, workspace })
      showRestoreConfirm.value = null
      return true
    } catch (e) {
      console.error('Failed to restore bookmark:', e)
      return false
    }
  }

  async function deleteBookmark(bookmarkId: number, sessionId: number) {
    try {
      await invoke('delete_bookmark', { bookmarkId })
      await loadBookmarks(sessionId)
    } catch (e) {
      console.error('Failed to delete bookmark:', e)
    }
  }

  return {
    bookmarks, showBookmarkPanel, showRestoreConfirm, showSaveInput, bookmarkName,
    loadBookmarks, saveBookmark, restoreBookmark, deleteBookmark,
  }
}
