<template>
  <div v-if="visible" class="cmd-popup">
    <div class="cmd-header">
      <span class="cmd-title">命令</span>
      <button class="cmd-close" @click="$emit('close')">&times;</button>
    </div>
    <div v-for="cmd in filteredCommands" :key="cmd.name" class="cmd-item"
      :class="{ active: selectedIndex === filteredCommands.indexOf(cmd) }"
      @click="$emit('select', cmd.name)">
      <span class="cmd-name">{{ cmd.name }}</span>
      <span class="cmd-desc">{{ cmd.desc }}</span>
    </div>
    <div v-if="filteredCommands.length === 0" class="cmd-empty">无匹配命令</div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export interface CommandItem {
  name: string
  desc: string
}

const props = defineProps<{
  visible: boolean
  query: string
  commands: CommandItem[]
  selectedIndex: number
}>()

defineEmits<{
  (e: 'select', name: string): void
  (e: 'close'): void
}>()

const filteredCommands = computed(() => {
  const q = props.query.toLowerCase()
  return props.commands.filter(c => c.name.toLowerCase().startsWith(q))
})
</script>

<style scoped>
.cmd-popup {
  background: var(--bg-secondary); border: 1px solid var(--border-color);
  border-radius: 8px; overflow: hidden;
}
.cmd-header {
  display: flex; justify-content: space-between; align-items: center;
  padding: 8px 12px; border-bottom: 1px solid var(--border-color);
}
.cmd-title { font-weight: 500; font-size: 13px; color: var(--text-secondary); }
.cmd-close { background: none; border: none; color: var(--text-secondary); cursor: pointer; font-size: 16px; }
.cmd-item {
  display: flex; justify-content: space-between; align-items: center;
  padding: 8px 12px; cursor: pointer;
}
.cmd-item.active, .cmd-item:hover { background: var(--bg-tertiary); }
.cmd-name { font-weight: 500; color: var(--text-primary); }
.cmd-desc { font-size: 12px; color: var(--text-secondary); }
.cmd-empty { padding: 8px 12px; color: var(--text-secondary); font-size: 13px; }
</style>
