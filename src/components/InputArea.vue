<template>
  <div class="input-area">
    <button class="attach-btn" @click="onAttachment" title="上传附件">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14 8l-6 6a4 4 0 01-6-6l6-6a2.5 2.5 0 014 4L8 12a1 1 0 01-2 0L8 8" />
      </svg>
    </button>
    <input
      type="text"
      v-model="inputText"
      placeholder="输入消息..."
      @keyup.enter="onSend"
    />
    <button class="send-btn" @click="onSend" title="发送">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14 2L7 9M14 2l-5 12-2-5-5-2 12-5z" />
      </svg>
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const inputText = ref('')

const onAttachment = () => {
  console.log('Attachment clicked')
}

const onSend = () => {
  if (inputText.value.trim()) {
    console.log('Send:', inputText.value)
    inputText.value = ''
  }
}
</script>

<style scoped>
.input-area {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background-color: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
}

.attach-btn {
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 4px;
  background-color: var(--btn-attachment);
  font-size: 16px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.attach-btn:hover {
  background-color: var(--btn-attachment-hover);
}

input {
  flex: 1;
  height: 36px;
  padding: 0 12px;
  background-color: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 14px;
  outline: none;
}

input:focus {
  border-color: var(--accent);
}

input::placeholder {
  color: var(--text-secondary);
}

.send-btn {
  width: 60px;
  height: 36px;
  border: none;
  border-radius: 4px;
  background-color: var(--btn-run);
  color: white;
  font-size: 14px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.send-btn:hover {
  background-color: var(--btn-run-hover);
}
</style>