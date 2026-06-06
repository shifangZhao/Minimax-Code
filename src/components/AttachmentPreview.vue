<template>
  <div v-if="files.length > 0" class="att-preview-strip">
    <div v-for="(att, i) in files" :key="i" class="att-preview-item">
      <span class="att-preview-name">{{ att.name }}</span>
      <button class="att-preview-remove" @click="$emit('remove', i)" :title="'Remove ' + att.name">x</button>
    </div>
  </div>
</template>

<script setup lang="ts">
export interface AttInfo {
  name: string
  path: string
  kind: 'image' | 'file' | 'text'
  content?: string
}

defineProps<{
  files: AttInfo[]
}>()

defineEmits<{
  (e: 'remove', index: number): void
}>()
</script>

<style scoped>
.att-preview-strip {
  display: flex; gap: 8px; flex-wrap: wrap;
  padding: 6px 12px; border-top: 1px solid var(--border-color);
}
.att-preview-item {
  display: flex; align-items: center; gap: 6px;
  background: var(--bg-tertiary); border-radius: 6px;
  padding: 4px 8px; font-size: 13px;
}
.att-preview-name {
  color: var(--text-primary); max-width: 180px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.att-preview-remove {
  background: none; border: none; color: var(--text-secondary);
  cursor: pointer; font-size: 14px; padding: 0; line-height: 1;
}
.att-preview-remove:hover { color: var(--accent-warn); }
</style>
