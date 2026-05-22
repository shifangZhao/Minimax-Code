<template>
  <div class="title-bar" data-tauri-drag-region>
    <div class="title-left" data-tauri-drag-region>
      <img src="../assets/icon.png" class="title-icon" alt="" />
      <span class="title">MinimaxCode</span>
    </div>
    <div class="window-controls">
      <button class="control-btn settings" @click="$emit('openSettings')" title="设置">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="3"/><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/></svg>
      </button>
      <button class="control-btn minimize" @click="minimize" title="最小化">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="5" y1="12" x2="19" y2="12"/></svg>
      </button>
      <button class="control-btn maximize" @click="maximize" title="最大化">
        <svg v-if="isMaximized" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><rect x="4" y="4" width="16" height="16" rx="2"/><line x1="4" y1="7" x2="20" y2="7"/></svg>
        <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><rect x="4" y="4" width="16" height="16" rx="2"/></svg>
      </button>
      <button class="control-btn close" @click="close" title="关闭">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

defineEmits<{
  (e: 'openSettings'): void
}>()

const isMaximized = ref(false)

const minimize = async () => {
  await invoke('minimize_window')
}

const maximize = async () => {
  await invoke('maximize_window')
  isMaximized.value = await invoke('is_maximized')
}

const close = async () => {
  await invoke('close_window')
}

const checkMaximized = async () => {
  isMaximized.value = await invoke('is_maximized')
}

onMounted(() => {
  checkMaximized()
})
</script>

<style scoped>
.title-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  height: 40px;
  background-color: var(--bg-secondary);
  padding: 0 8px;
  border-bottom: 1px solid var(--border-color);
  -webkit-app-region: drag;
  user-select: none;
}

.title-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.title-icon {
  width: 20px;
  height: 20px;
}

.title {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.window-controls {
  display: flex;
  gap: 2px;
  -webkit-app-region: no-drag;
}

.control-btn {
  width: 36px;
  height: 36px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  transition: background-color 0.15s, color 0.15s;
}

.control-btn:hover {
  background-color: var(--bg-tertiary);
  color: var(--text-primary);
}

.control-btn.close:hover {
  background-color: #e81123;
  color: white;
}
</style>