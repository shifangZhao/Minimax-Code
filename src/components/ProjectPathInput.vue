<template>
  <div class="project-path">
    <span class="label">工作目录：</span>
    <input
      type="text"
      v-model="projectPath"
      placeholder="输入路径或选择文件夹..."
      @blur="savePath"
    />
    <button class="file-btn" @click="selectFolder" title="选择文件夹">📁</button>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'

const projectPath = ref('')

onMounted(async () => {
  try {
    const workspace = await invoke<string>('get_workspace')
    if (workspace) {
      projectPath.value = workspace
    }
  } catch (e) {
    console.error('Failed to load workspace:', e)
  }
})

const savePath = async () => {
  if (projectPath.value.trim()) {
    try {
      await invoke('set_workspace', { workspace: projectPath.value.trim() })
    } catch (e) {
      console.error('Failed to save workspace:', e)
    }
  }
}

const selectFolder = async () => {
  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择项目文件夹'
  })
  if (selected) {
    projectPath.value = selected as string
    try {
      await invoke('set_workspace', { workspace: selected })
    } catch (e) {
      console.error('Failed to save workspace:', e)
    }
  }
}
</script>

<style scoped>
.project-path {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  background-color: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
}

.label {
  color: var(--text-secondary);
  font-size: 13px;
  white-space: nowrap;
}

input {
  flex: 1;
  height: 28px;
  padding: 0 10px;
  background-color: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
}

input:focus {
  border-color: var(--accent);
}

.file-btn {
  width: 32px;
  height: 28px;
  border: none;
  background-color: var(--bg-tertiary);
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  transition: background-color 0.15s;
}

.file-btn:hover {
  background-color: var(--accent);
}
</style>