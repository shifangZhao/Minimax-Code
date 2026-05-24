<template>
  <div class="perm-card">
    <div class="perm-tool">{{ request.tool }}</div>
    <div class="perm-reason">{{ request.reason }}</div>
    <div class="perm-actions">
      <button class="perm-deny" @click="$emit('deny')">拒绝</button>
      <button class="perm-allow-once" @click="$emit('allow')">允许一次</button>
      <button class="perm-allow-always" @click="$emit('allowAlways')">始终允许</button>
    </div>
  </div>
</template>

<script setup lang="ts">
export interface PermissionRequestInfo {
  id: string
  tool: string
  reason: string
  file?: string
  command?: string
}

defineProps<{
  request: PermissionRequestInfo
}>()

defineEmits<{
  (e: 'allow'): void
  (e: 'deny'): void
  (e: 'allowAlways'): void
}>()
</script>

<style scoped>
.perm-card {
  background: var(--bg-secondary); border: 1px solid var(--accent-warn);
  border-radius: 8px; padding: 12px; margin-bottom: 8px;
}
.perm-tool {
  font-weight: 500; color: var(--accent-warn); margin-bottom: 6px;
}
.perm-reason {
  font-size: 13px; color: var(--text-secondary); margin-bottom: 10px;
}
.perm-actions {
  display: flex; gap: 8px;
}
.perm-deny, .perm-allow-once, .perm-allow-always {
  padding: 6px 12px; border: none; border-radius: 6px;
  font-size: 13px; cursor: pointer;
}
.perm-deny { background: var(--bg-tertiary); color: var(--text-primary); }
.perm-allow-once { background: var(--btn-run); color: white; }
.perm-allow-always { background: var(--accent-warn); color: white; }
</style>
