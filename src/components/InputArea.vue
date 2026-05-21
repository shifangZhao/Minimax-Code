<template>
  <div class="input-area">
    <button class="attach-btn" @click="onAttachment" title="上传附件">📎</button>
    <input
      type="text"
      v-model="inputText"
      placeholder="输入消息..."
      @keyup.enter="onSend"
    />
    <button class="send-btn" @click="onSend" title="发送">🚀</button>
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