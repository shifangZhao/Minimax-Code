<template>
  <div class="history-sidebar">
    <div class="sidebar-header">
      <span class="sidebar-title">{{ currentMode === 'ace' ? '📋 会话' : '📋 群聊' }}</span>
      <button class="add-btn" @click.stop="createGroupChat" :title="currentMode === 'ace' ? '新建会话' : '新建群聊'">+</button>
    </div>
    <div class="sidebar-content">
      <div v-if="loading" class="loading">加载中...</div>
      <div v-else-if="groupChats.length === 0" class="empty">{{ currentMode === 'ace' ? '暂无会话' : '暂无群聊' }}</div>
      <div v-else class="chat-list">
        <div
          v-for="chat in groupChats"
          :key="chat.id"
          class="chat-item"
          :class="{ active: activeChatId === chat.id }"
          @click="selectGroupChat(chat.id)"
          @dblclick="startRename(chat)"
        >
          <span class="chat-icon">💬</span>
          <input
            v-if="editingId === chat.id"
            v-model="editingName"
            class="chat-name-input"
            @blur="confirmRename"
            @keyup.enter="confirmRename"
            @keyup.esc="cancelRename"
            @click.stop
            ref="editInput"
          />
          <span v-else class="chat-name">{{ chat.name }}</span>
          <button class="delete-btn" :class="{ confirm: confirmDeleteId === chat.id }" @click.stop="handleDeleteClick(chat.id)" title="删除">🗑️</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from 'vue'
import { useRoute } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { db } from '../services/db'

const route = useRoute()
const currentMode = computed(() => route.path.startsWith('/ace') ? 'ace' : 'team')

interface GroupChat {
  id: number
  name: string
  mode: string
  created_at: string
  temporary?: boolean
}

const loading = ref(false)
const groupChats = ref<GroupChat[]>([])
const activeChatId = ref<number | null>(null)
const nextTemporaryId = ref(-1)

const emit = defineEmits<{
  (e: 'selectGroupChat', chatId: number): void
}>()

const editingId = ref<number | null>(null)
const editingName = ref('')
const editInput = ref<HTMLInputElement | null>(null)
const confirmDeleteId = ref<number | null>(null)

const loadGroupChats = async () => {
  loading.value = true
  try {
    const chats = await db.getGroupChats(currentMode.value)
    groupChats.value = chats

    // Auto-select first chat if none selected
    if (chats.length > 0 && activeChatId.value === null) {
      selectGroupChat(chats[0].id)
    }
  } catch (e) {
    console.error('Failed to load group chats:', e)
  } finally {
    loading.value = false
  }
}

// Reload chats when mode switches
watch(currentMode, () => {
  activeChatId.value = null
  loadGroupChats()
  emit('selectGroupChat', null as any)
})

const createGroupChat = () => {
  // Create a temporary chat locally, will be persisted to DB only when user sends first message
  const tempChat: GroupChat = {
    id: nextTemporaryId.value--,
    name: currentMode.value === 'ace' ? 'Ace 对话' : '新群聊',
    mode: currentMode.value,
    created_at: new Date().toISOString(),
    temporary: true,
  }
  groupChats.value.unshift(tempChat)
  activeChatId.value = tempChat.id
  emit('selectGroupChat', tempChat.id)
}

const handleDeleteClick = async (id: number) => {
  if (confirmDeleteId.value === id) {
    // Second click - execute delete
    const chat = groupChats.value.find(c => c.id === id)
    if (chat?.temporary) {
      // Just remove from local list
      groupChats.value = groupChats.value.filter(c => c.id !== id)
      if (activeChatId.value === id) {
        activeChatId.value = groupChats.value[0]?.id ?? null
        emit('selectGroupChat', activeChatId.value)
      }
    } else {
      // Physical delete from DB
      try {
        await invoke('delete_group_chat', { id })
        groupChats.value = groupChats.value.filter(c => c.id !== id)
        if (activeChatId.value === id) {
          activeChatId.value = groupChats.value[0]?.id ?? null
          emit('selectGroupChat', activeChatId.value)
        }
      } catch (e) {
        console.error('Failed to delete group chat:', e)
      }
    }
    confirmDeleteId.value = null
  } else {
    // First click - show confirm state
    confirmDeleteId.value = id
  }
}

const selectGroupChat = (chatId: number) => {
  activeChatId.value = chatId
  emit('selectGroupChat', chatId)
}

const startRename = (chat: GroupChat) => {
  editingId.value = chat.id
  editingName.value = chat.name
  nextTick(() => {
    editInput.value?.focus()
  })
}

const confirmRename = async () => {
  if (editingId.value && editingName.value.trim()) {
    const chat = groupChats.value.find(c => c.id === editingId.value)
    if (chat) chat.name = editingName.value.trim()

    // Only call DB if it's not a temporary chat (positive ID)
    if (editingId.value > 0) {
      try {
        await db.renameGroupChat(editingId.value, editingName.value.trim())
      } catch (e) {
        console.error('Failed to rename group chat:', e)
      }
    }
  }
  editingId.value = null
}

const cancelRename = () => {
  editingId.value = null
}

defineExpose({
  loadGroupChats,
})

onMounted(async () => {
  await loadGroupChats()
})
</script>

<style scoped>
.history-sidebar {
  display: flex;
  flex-direction: column;
  background-color: var(--bg-secondary);
  border-right: 1px solid var(--border-color);
  width: 200px;
}

.sidebar-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px;
  border-bottom: 1px solid var(--border-color);
}

.sidebar-title {
  flex: 1;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
}

.add-btn {
  width: 20px;
  height: 20px;
  border: none;
  background-color: var(--accent);
  color: white;
  border-radius: 4px;
  font-size: 14px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.sidebar-content {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
}

.loading,
.empty {
  padding: 16px 12px;
  font-size: 13px;
  color: var(--text-secondary);
  text-align: center;
}

.chat-list {
  display: flex;
  flex-direction: column;
}

.chat-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  cursor: pointer;
}

.chat-item:hover {
  background-color: var(--bg-tertiary);
}

.chat-item.active {
  background-color: var(--bg-tertiary);
  border-left: 2px solid var(--accent);
}

.chat-icon {
  font-size: 14px;
}

.chat-name {
  flex: 1;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.chat-name-input {
  flex: 1;
  font-size: 13px;
  color: var(--text-primary);
  background: var(--bg-input);
  border: 1px solid var(--accent);
  border-radius: 3px;
  padding: 2px 6px;
  outline: none;
}

.delete-btn {
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  border-radius: 4px;
  display: none;
  align-items: center;
  justify-content: center;
}

.chat-item:hover .delete-btn {
  display: flex;
}

.delete-btn.confirm {
  background-color: #e81123;
  color: white;
  display: flex;
}
</style>