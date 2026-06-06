<template>
  <div class="bg-task-panel" v-if="tasks.size > 0">
    <div class="bg-task-header" @click="collapsed = !collapsed">
      <span class="bg-task-title">后台任务 ({{ tasks.size }})</span>
      <span class="bg-task-memory" v-if="memoryBytes > 0">{{ memoryMB }} MB</span>
      <span class="bg-task-toggle">
        <svg v-if="collapsed" width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M5 3l5 4-5 4" />
        </svg>
        <svg v-else width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M3 5l4 5 4-5" />
        </svg>
      </span>
    </div>
    <div class="bg-task-list" v-if="!collapsed">
      <div
        v-for="[id, task] in sortedTasks"
        :key="id"
        class="bg-task-item"
        :class="{ running: task.running, done: !task.running }"
      >
        <div class="bg-task-row" @click="toggleCollapse(id)">
          <span class="bg-task-status" :title="task.running ? '运行中' : '已结束'">
            <svg v-if="task.running" width="10" height="10" viewBox="0 0 14 14" fill="currentColor">
              <circle cx="7" cy="7" r="5" />
            </svg>
            <svg v-else width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5">
              <circle cx="7" cy="7" r="5" />
            </svg>
          </span>
          <span class="bg-task-command" :title="task.command">{{ truncateCmd(task.command) }}</span>
          <span class="bg-task-pid">PID {{ task.pid }}</span>
          <span class="bg-task-time">{{ formatTime(task.start_time) }}</span>
          <span class="bg-task-arrow">
            <svg v-if="task.collapsed" width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 3l5 4-5 4" />
            </svg>
            <svg v-else width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 5l4 5 4-5" />
            </svg>
          </span>
          <button
            class="bg-task-kill"
            :title="task.running ? '强制终止' : '从列表移除'"
            @click.stop="task.running ? killTask(id) : removeTask(id)"
          >
            <svg width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M1 1l12 12M13 1L1 13" />
            </svg>
          </button>
        </div>
        <div class="bg-task-output" v-if="!task.collapsed" ref="outputEls">
          <pre class="bg-task-output-text">{{ task.output || '(暂无输出)' }}</pre>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useBackgroundTasks } from '../composables/useBackgroundTasks'

const { tasks, killTask, removeTask, loadFullOutput, toggleCollapse } = useBackgroundTasks()

const collapsed = ref(false)
const memoryBytes = ref(0)
const memoryMB = computed(() => memoryBytes.value > 0 ? (memoryBytes.value / 1048576).toFixed(1) : '0')

let memTimer: ReturnType<typeof setInterval> | null = null
async function refreshMemory() {
  try {
    const stats: { total_memory_bytes: number } = await invoke('get_process_stats')
    memoryBytes.value = stats.total_memory_bytes || 0
  } catch { /* ignore */ }
}
onMounted(() => { refreshMemory(); memTimer = setInterval(refreshMemory, 5000) })
onUnmounted(() => { if (memTimer) clearInterval(memTimer) })

const sortedTasks = computed(() => {
  const arr = [...tasks.value.entries()]
  arr.sort((a, b) => b[1].start_time - a[1].start_time)
  return arr
})

function truncateCmd(cmd: string): string {
  return cmd.length > 60 ? cmd.slice(0, 57) + '...' : cmd
}

function formatTime(ts: number): string {
  if (!ts) return ''
  const seconds = Math.floor(Date.now() / 1000) - ts
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`
  return `${Math.floor(seconds / 3600)}h`
}

// Auto-load full output when expanding a task
watch(tasks, (newVal) => {
  for (const [id, task] of newVal) {
    // If a task was just created, we don't need to load output (events will stream)
    // But if expanding an existing task, ensure it has output loaded
    if (!task.collapsed && !task.output && task.out_file) {
      loadFullOutput(id)
    }
  }
}, { deep: true })
</script>

<style scoped>
.bg-task-panel {
  border-top: 1px solid var(--border-color, #333);
  background: var(--bg-secondary, #1a1a2e);
  font-size: 12px;
  user-select: none;
}
.bg-task-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 6px 12px;
  cursor: pointer;
  color: var(--text-muted, #888);
}
.bg-task-header:hover {
  background: var(--bg-hover, #252540);
}
.bg-task-title {
  font-weight: 600;
}
.bg-task-memory {
  color: var(--text-muted, #888);
  font-size: 11px;
  font-family: var(--font-mono, 'Courier New', monospace);
}
.bg-task-toggle {
  font-size: 10px;
}
.bg-task-list {
  max-height: 300px;
  overflow-y: auto;
}
.bg-task-item {
  border-top: 1px solid var(--border-color, #333);
}
.bg-task-item.running {
  border-left: 2px solid #4caf50;
}
.bg-task-item.done {
  border-left: 2px solid #666;
  opacity: 0.7;
}
.bg-task-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 12px;
  cursor: pointer;
}
.bg-task-row:hover {
  background: var(--bg-hover, #252540);
}
.bg-task-status {
  font-size: 8px;
  flex-shrink: 0;
}
.bg-task-command {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary, #ddd);
  font-family: var(--font-mono, 'Courier New', monospace);
}
.bg-task-pid {
  color: var(--text-muted, #888);
  flex-shrink: 0;
  font-size: 11px;
}
.bg-task-time {
  color: var(--text-muted, #888);
  flex-shrink: 0;
  font-size: 11px;
  min-width: 30px;
  text-align: right;
}
.bg-task-arrow {
  font-size: 8px;
  color: var(--text-muted, #888);
  flex-shrink: 0;
}
.bg-task-kill {
  background: none;
  border: 1px solid #e53935;
  color: #e53935;
  border-radius: 3px;
  cursor: pointer;
  font-size: 10px;
  padding: 1px 5px;
  flex-shrink: 0;
}
.bg-task-kill:hover {
  background: #e53935;
  color: #fff;
}
.bg-task-output {
  padding: 0 12px 8px;
}
.bg-task-output-text {
  margin: 0;
  padding: 6px 8px;
  background: #0d0d0d;
  border-radius: 4px;
  color: #ccc;
  font-size: 11px;
  font-family: var(--font-mono, 'Courier New', monospace);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 200px;
  overflow-y: auto;
  line-height: 1.4;
}
</style>
