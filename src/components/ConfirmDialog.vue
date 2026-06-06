<template>
  <div v-if="visible" class="confirm-overlay" @click.self="$emit('cancel')">
    <div class="confirm-dialog">
      <div class="confirm-header">
        <div class="confirm-title">{{ title }}</div>
        <button class="confirm-close" @click="$emit('cancel')">&times;</button>
      </div>
      <div class="confirm-message">{{ message }}</div>
      <div class="confirm-btns">
        <button class="cf-cancel-btn" @click="$emit('cancel')">{{ cancelText }}</button>
        <button :class="danger ? 'cf-danger-btn' : 'cf-confirm-btn'" @click="$emit('confirm')">{{ confirmText }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
withDefaults(defineProps<{
  visible: boolean
  title: string
  message: string
  confirmText?: string
  cancelText?: string
  danger?: boolean
}>(), {
  confirmText: '确认',
  cancelText: '取消',
  danger: false,
})

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
.confirm-header {
  display: flex; justify-content: space-between; align-items: center;
  margin-bottom: 12px;
}
.confirm-title {
  font-size: 16px; font-weight: 500;
  color: var(--text-primary);
}
.confirm-close {
  width: 24px; height: 24px; border: none; background: transparent;
  color: var(--text-secondary); font-size: 18px; cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  border-radius: 4px; line-height: 1;
}
.confirm-close:hover { background: var(--bg-tertiary); color: var(--text-primary); }
.confirm-message {
  font-size: 14px;
  color: var(--text-secondary);
  margin-bottom: 20px;
  word-break: break-all;
}
.confirm-btns {
  display: flex; justify-content: flex-end; gap: 10px;
}
.cf-cancel-btn, .cf-confirm-btn, .cf-danger-btn {
  padding: 8px 16px; border: none; border-radius: 6px;
  font-size: 14px; cursor: pointer;
}
.cf-cancel-btn { background: var(--bg-tertiary); color: var(--text-primary); }
.cf-confirm-btn { background: var(--btn-run); color: white; }
.cf-danger-btn { background: #dc3545; color: white; }
.cf-danger-btn:hover { background: #c82333; }
</style>
