<template>
  <Transition name="toast-slide">
    <div v-if="visible" class="toast-bar">
      <span class="toast-text">{{ message }}</span>
      <button v-if="actionLabel" class="toast-action" @click="$emit('action')">{{ actionLabel }}</button>
      <button class="toast-close" @click="$emit('close')">×</button>
    </div>
  </Transition>
</template>

<script setup lang="ts">
defineProps<{
  visible: boolean
  message: string
  actionLabel?: string
}>()

defineEmits<{
  (e: 'action'): void
  (e: 'close'): void
}>()
</script>

<style scoped>
.toast-bar {
  position: absolute;
  bottom: 16px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 16px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  z-index: 100;
  box-shadow: 0 4px 12px var(--shadow);
  max-width: 90%;
}

.toast-text {
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toast-action {
  padding: 4px 12px;
  border: none;
  background: var(--accent);
  color: white;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
}

.toast-action:hover {
  background: var(--accent-hover);
}

.toast-close {
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 14px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
}

.toast-close:hover {
  background: var(--bg-input);
  color: var(--text-primary);
}

.toast-slide-enter-active,
.toast-slide-leave-active {
  transition: all 0.25s ease;
}

.toast-slide-enter-from,
.toast-slide-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(12px);
}
</style>
