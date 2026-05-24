<template>
  <div class="title-bar" data-tauri-drag-region>
    <div class="title-left" data-tauri-drag-region>
      <img src="../assets/icon.png" class="title-icon" alt="" />
      <span class="title">MinimaxCode</span>
      <button class="sidebar-toggle-btn" @click="$emit('toggleSidebar')" title="切换侧边栏">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="11" height="11" rx="1.5"/>
          <rect x="10" y="9" width="11" height="11" rx="1.5"/>
        </svg>
      </button>
    </div>
    <div class="window-controls">
      <ModeSwitcher class="inline-mode-switcher" />
      <div class="theme-picker">
        <button class="control-btn theme" @click="themeOpen = !themeOpen" @mouseenter="openTheme" @mouseleave="scheduleClose" title="主题">
          <svg v-if="theme === 'dark'" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"/></svg>
          <svg v-else-if="theme === 'light'" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
          <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="12" cy="12" r="5"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M5.6 18.4l1.4-1.4M17 7l1.4-1.4"/></svg>
        </button>
        <div v-show="themeOpen" class="theme-dropdown" @mouseenter="openTheme" @mouseleave="scheduleClose">
          <div class="theme-option" :class="{ active: theme === 'dark' }" @click="pickTheme('dark')">
            <span class="theme-dot dark-dot"></span> 深色
          </div>
          <div class="theme-option" :class="{ active: theme === 'light' }" @click="pickTheme('light')">
            <span class="theme-dot light-dot"></span> 浅色
          </div>
          <div class="theme-option" :class="{ active: theme === 'warm' }" @click="pickTheme('warm')">
            <span class="theme-dot warm-dot"></span> 暖色
          </div>
        </div>
      </div>
      <button class="control-btn settings" @click="$emit('openSettings')" title="设置">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
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
import ModeSwitcher from './ModeSwitcher.vue'

defineEmits<{
  (e: 'openSettings'): void
  (e: 'toggleSidebar'): void
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

const themeOpen = ref(false)
const theme = ref(localStorage.getItem('theme') || 'dark')
let closeTimer: ReturnType<typeof setTimeout> | null = null

function openTheme() {
  if (closeTimer) { clearTimeout(closeTimer); closeTimer = null }
  themeOpen.value = true
}

function scheduleClose() {
  closeTimer = setTimeout(() => { themeOpen.value = false }, 150)
}

function pickTheme(t: string) {
  theme.value = t
  document.documentElement.setAttribute('data-theme', t)
  localStorage.setItem('theme', t)
  themeOpen.value = false
}

// Apply saved theme on load
if (theme.value !== 'dark') {
  document.documentElement.setAttribute('data-theme', theme.value)
}
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

.sidebar-toggle-btn {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  -webkit-app-region: no-drag;
  transition: background-color 0.15s, color 0.15s;
}

.sidebar-toggle-btn:hover {
  background-color: var(--bg-tertiary);
  color: var(--text-primary);
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

.inline-mode-switcher {
  margin: 0 4px;
}

.inline-mode-switcher :deep(.mode-switcher) {
  margin: 0;
  padding: 2px;
}

.inline-mode-switcher :deep(.mode-btn) {
  padding: 4px 10px;
  font-size: 12px;
}

.theme-picker {
  position: relative;
}

.theme-dropdown {
  position: absolute;
  top: 38px;
  right: 0;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 4px;
  z-index: 100;
  min-width: 90px;
  box-shadow: 0 4px 12px var(--shadow);
}

.theme-option {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  color: var(--text-primary);
  white-space: nowrap;
}

.theme-option:hover {
  background: var(--bg-tertiary);
}

.theme-option.active {
  color: var(--accent);
}

.theme-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dark-dot  { background: #5078b0; }
.light-dot { background: #2563eb; }
.warm-dot  { background: #f97316; }

[data-theme="warm"] .dark-dot  { background: #0f0c08; }
[data-theme="warm"] .light-dot { background: #f0e0c8; }
[data-theme="warm"] .warm-dot  { background: #f97316; }
</style>