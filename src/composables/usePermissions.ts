import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface PermRequest {
  id: string
  tool: string
  reason: string
  file?: string
  command?: string
}

const permRequests = ref<PermRequest[]>([])

export function usePermissions() {
  async function respond(req: PermRequest, action: string, always: boolean) {
    await invoke('respond_permission', { id: req.id, tool: req.tool, action, always })
    permRequests.value = permRequests.value.filter(r => r.id !== req.id)
  }

  return {
    permRequests,
    respond,
  }
}
