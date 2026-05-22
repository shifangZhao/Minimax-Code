<template>
  <div v-if="visible" class="bookmark-panel">
    <div class="bm-header">
      <span class="bm-title">快照</span>
      <button class="bm-save-btn" @click="onSaveClick">{{ showInput ? '取消' : '+ 新快照' }}</button>
    </div>

    <div v-if="showInput" class="bm-save-row">
      <input
        v-model="name"
        class="bm-input"
        placeholder="快照名称（可选）"
        @keyup.enter="$emit('save', name)"
      />
      <button class="bm-confirm" @click="$emit('save', name)">保存</button>
    </div>

    <div v-if="items.length === 0" class="bm-empty">暂无快照</div>

    <div v-for="bm in items" :key="bm.id" class="bm-item">
      <div class="bm-info">
        <span class="bm-name">{{ bm.name }}</span>
        <span class="bm-meta">{{ bm.message_count }} 条消息 · {{ formatSize(bm.total_bytes) }}</span>
      </div>
      <div class="bm-actions">
        <button class="bm-restore" @click="$emit('restore', bm)" title="恢复">↩</button>
        <button class="bm-delete" @click="$emit('delete', bm.id)" title="删除">×</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

defineProps<{
  visible: boolean
  items: Array<{
    id: number
    name: string
    message_count: number
    total_bytes: number
  }>
  showInput: boolean
}>()

const emit = defineEmits<{
  (e: 'save', name: string): void
  (e: 'restore', item: any): void
  (e: 'delete', id: number): void
  (e: 'toggleInput'): void
}>()

const name = ref('')

function onSaveClick() {
  name.value = ''
  emit('toggleInput')
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
</script>

<style scoped>
.bookmark-panel {
  position: absolute;
  top: 40px;
  right: 12px;
  width: 280px;
  max-height: 360px;
  overflow-y: auto;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  z-index: 50;
  box-shadow: 0 4px 16px var(--shadow);
}

.bm-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border-color);
}

.bm-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.bm-save-btn {
  padding: 3px 10px;
  border: 1px solid var(--border-color);
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border-radius: 4px;
  font-size: 11px;
  cursor: pointer;
}

.bm-save-btn:hover {
  background: var(--bg-input);
  color: var(--text-primary);
}

.bm-save-row {
  display: flex;
  gap: 6px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
}

.bm-input {
  flex: 1;
  height: 28px;
  padding: 0 8px;
  background: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 12px;
  outline: none;
}

.bm-input:focus {
  border-color: var(--accent);
}

.bm-confirm {
  padding: 3px 10px;
  border: none;
  background: var(--btn-run);
  color: white;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}

.bm-empty {
  padding: 20px 12px;
  text-align: center;
  font-size: 12px;
  color: var(--text-secondary);
}

.bm-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
}

.bm-item:last-child { border-bottom: none; }

.bm-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow: hidden;
}

.bm-name {
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.bm-meta {
  font-size: 11px;
  color: var(--text-secondary);
}

.bm-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
}

.bm-restore, .bm-delete {
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 14px;
  cursor: pointer;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.bm-restore:hover { background: var(--bg-input); color: var(--accent); }
.bm-delete:hover { background: var(--bg-input); color: #e81123; }
</style>
