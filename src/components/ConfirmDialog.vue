<template>
  <div v-if="visible" class="confirm-overlay" @click.self="$emit('cancel')">
    <div class="confirm-dialog">
      <div class="confirm-title">{{ title }}</div>
      <div class="confirm-message">{{ message }}</div>
      <div class="confirm-btns">
        <button class="cf-cancel-btn" @click="$emit('cancel')">{{ cancelText }}</button>
        <button class="cf-confirm-btn" @click="$emit('confirm')">{{ confirmText }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  visible: boolean
  title: string
  message: string
  confirmText?: string
  cancelText?: string
}>()

defineEmits<{
  (e: 'confirm'): void
  (e: 'cancel'): void
}>()
</script>

<style scoped>
.confirm-overlay {
  position: fixed; top: 0; left: 0; right: 0; bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex; align-items: center; justify-content: center;
  z-index: 1000;
}
.confirm-dialog {
  background: var(--bg-secondary);
  border-radius: 12px;
  padding: 20px 24px;
  width: 360px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
}
.confirm-title {
  font-size: 16px; font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 12px;
}
.confirm-message {
  font-size: 14px;
  color: var(--text-secondary);
  margin-bottom: 20px;
  word-break: break-all;
}
.confirm-btns {
  display: flex; justify-content: flex-end; gap: 10px;
}
.cf-cancel-btn, .cf-confirm-btn {
  padding: 8px 16px; border: none; border-radius: 6px;
  font-size: 14px; cursor: pointer;
}
.cf-cancel-btn { background: var(--bg-tertiary); color: var(--text-primary); }
.cf-confirm-btn { background: var(--btn-run); color: white; }
</style>
