<template>
  <div v-if="state && state.items.length > 0 && !hidden" class="todo-panel">
    <div class="todo-header" @click="expanded = !expanded">
      <span class="todo-icon">{{ expanded ? '▼' : '▶' }}</span>
      <span class="todo-summary">{{ state.summary }}</span>
      <span class="todo-pct">{{ state.pct }}%</span>
    </div>
    <div v-if="expanded" class="todo-list">
      <div
        v-for="(item, i) in state.items"
        :key="i"
        class="todo-item"
        :class="item.status"
      >
        <span class="todo-status-icon">
          <template v-if="item.status === 'completed'">✅</template>
          <template v-else-if="item.status === 'in_progress'">🔄</template>
          <template v-else>⏳</template>
        </span>
        <span class="todo-content">
          <template v-if="item.status === 'in_progress' && item.activeForm">
            {{ item.activeForm }}
          </template>
          <template v-else>
            {{ item.content }}
          </template>
        </span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useTodoStore, type TodoState } from '../composables/useTodoStore'

const props = defineProps<{
  sessionId: number | null
}>()

const { todoStates } = useTodoStore()

const state = computed<TodoState | null>(() => {
  if (props.sessionId === null) return null
  return todoStates.value.get(props.sessionId) ?? null
})

const expanded = ref(true)
const hidden = ref(false)

// Auto-hide 5s after all tasks completed.
// Tracks per-session "already hidden" state so switching back to a completed
// session doesn't flash the panel.
let hideTimer: ReturnType<typeof setTimeout> | null = null
const completedSessions = new Set<number>()

watch(state, (newState) => {
  if (hideTimer) {
    clearTimeout(hideTimer)
    hideTimer = null
  }

  const sid = props.sessionId
  if (!newState || newState.items.length === 0) {
    if (sid != null) completedSessions.delete(sid)
    hidden.value = false
    return
  }

  const allCompleted = newState.items.every(i => i.status === 'completed')

  if (allCompleted) {
    if (sid != null && completedSessions.has(sid)) {
      // Already hidden for this session — stay hidden, no flash
      hidden.value = true
    } else {
      // First time all-completed → show briefly, then hide
      hidden.value = false
      hideTimer = setTimeout(() => {
        hidden.value = true
        if (sid != null) completedSessions.add(sid)
      }, 5000)
    }
  } else {
    // New incomplete tasks → reset and show
    if (sid != null) completedSessions.delete(sid)
    hidden.value = false
  }
}, { deep: true, immediate: true })
</script>

<style scoped>
.todo-panel {
  margin: 0 16px 8px 16px;
  border: 1px solid var(--border-color, #333);
  border-radius: 8px;
  background: var(--panel-bg, #1a1a2e);
  overflow: hidden;
  font-size: 13px;
}

.todo-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  cursor: pointer;
  user-select: none;
  background: var(--panel-header-bg, #16213e);
}

.todo-header:hover {
  background: var(--panel-header-hover, #1a2744);
}

.todo-icon {
  font-size: 10px;
  color: var(--text-muted, #888);
  flex-shrink: 0;
  width: 12px;
}

.todo-summary {
  flex: 1;
  color: var(--text-primary, #ddd);
  font-weight: 500;
}

.todo-pct {
  color: var(--accent, #4fc3f7);
  font-weight: 600;
  font-size: 12px;
}

.todo-list {
  padding: 4px 12px 8px 12px;
}

.todo-item {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 4px 0;
  line-height: 1.4;
}

.todo-item.pending .todo-content {
  color: var(--text-muted, #888);
}

.todo-item.in_progress .todo-content {
  color: var(--text-primary, #ddd);
  font-weight: 500;
}

.todo-item.completed .todo-content {
  color: var(--text-success, #66bb6a);
}

.todo-status-icon {
  flex-shrink: 0;
  font-size: 12px;
  margin-top: 1px;
}
</style>
