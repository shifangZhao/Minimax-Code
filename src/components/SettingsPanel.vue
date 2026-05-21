<template>
  <div class="settings-overlay" v-if="visible" @click.self="close">
    <div class="settings-panel">
      <div class="panel-header">
        <h2>设置</h2>
        <button class="close-btn" @click="close">✕</button>
      </div>
      <div class="panel-content">
        <div class="form-group">
          <label>Minimax API Key</label>
          <div class="input-wrapper">
            <input
              :type="showKey ? 'text' : 'password'"
              v-model="apiKey"
              placeholder="输入 API Key..."
            />
            <button class="toggle-btn" @click="showKey = !showKey">
              {{ showKey ? '👁' : '👁‍🗨' }}
            </button>
          </div>
          <p class="hint">用于连接 MiniMax AI 服务</p>
        </div>

        <div class="form-group">
          <label>权限模式</label>
          <select v-model="permMode" class="model-select">
            <option value="full">Full — 全部自动放行</option>
            <option value="normal">Normal — 安全命令自动，危险确认</option>
            <option value="guarded">Guarded — 修改操作都需确认</option>
          </select>
          <p class="hint">Full 最省心，Guarded 最安全。敏感路径（.env, 密钥文件）始终拦截</p>
        </div>

        <div class="form-group">
          <label>模型选择</label>
          <select v-model="selectedModel" class="model-select">
            <option value="MiniMax-M2.7">MiniMax-M2.7 (204,800) - 开启模型的自我迭代</option>
            <option value="MiniMax-M2.7-highspeed">MiniMax-M2.7-highspeed (204,800) - M2.7 极速版</option>
            <option value="MiniMax-M2.5">MiniMax-M2.5 (204,800) - 顶尖性能与极致性价比</option>
            <option value="MiniMax-M2.5-highspeed">MiniMax-M2.5-highspeed (204,800) - M2.5 极速版</option>
            <option value="MiniMax-M2.1">MiniMax-M2.1 (204,800) - 强大多语言编程能力</option>
            <option value="MiniMax-M2.1-highspeed">MiniMax-M2.1-highspeed (204,800) - M2.1 极速版</option>
            <option value="MiniMax-M2">MiniMax-M2 (204,800) - 专为高效编码与 Agent 工作流而生</option>
          </select>
        </div>
      </div>
      <div class="panel-footer">
        <button class="save-btn" @click="saveAll" :disabled="saving">
          {{ saving ? '保存中...' : '保存' }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  (e: 'close'): void
}>()

const apiKey = ref('')
const selectedModel = ref('MiniMax-M2.7')
const permMode = ref('normal')
const saving = ref(false)
const showKey = ref(false)

watch(() => props.visible, async (val) => {
  if (val) {
    await loadSettings()
  }
})

const loadSettings = async () => {
  try {
    const key = await invoke<string | null>('get_minimax_api_key')
    apiKey.value = key || ''

    const model = await invoke<string>('get_model')
    selectedModel.value = model || 'MiniMax-M2.7'

    const mode = await invoke<string>('get_permission_mode')
    permMode.value = JSON.parse(mode) || 'normal'
  } catch (e) {
    console.error('Failed to load settings:', e)
  }
}

const saveAll = async () => {
  saving.value = true
  try {
    if (apiKey.value.trim()) {
      await invoke('set_minimax_api_key', { apiKey: apiKey.value })
    }
    await invoke('set_model', { model: selectedModel.value })
    await invoke('set_permission_mode', { mode: permMode.value })
    close()
  } catch (e) {
    console.error('Failed to save settings:', e)
  } finally {
    saving.value = false
  }
}

const close = () => {
  emit('close')
}
</script>

<style scoped>
.settings-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.settings-panel {
  width: 500px;
  background-color: var(--bg-secondary);
  border-radius: 8px;
  border: 1px solid var(--border-color);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-color);
}

.panel-header h2 {
  margin: 0;
  font-size: 16px;
  color: var(--text-primary);
}

.close-btn {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 14px;
  cursor: pointer;
  border-radius: 4px;
}

.close-btn:hover {
  background-color: var(--bg-tertiary);
  color: var(--text-primary);
}

.panel-content {
  padding: 20px;
}

.form-group {
  margin-bottom: 20px;
}

.form-group:last-child {
  margin-bottom: 0;
}

.form-group label {
  display: block;
  margin-bottom: 8px;
  font-size: 13px;
  color: var(--text-primary);
}

.input-wrapper {
  position: relative;
}

.input-wrapper input {
  width: 100%;
  height: 36px;
  padding: 0 40px 0 12px;
  background-color: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 14px;
  -webkit-text-security: none;
}

.input-wrapper input::-webkit-credentials-auto-fill-button {
  display: none !important;
}

.input-wrapper input::-webkit-caps-lock-indicator {
  display: none !important;
}

.input-wrapper input:focus {
  outline: none;
  border-color: var(--accent);
}

.toggle-btn {
  position: absolute;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  font-size: 16px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
}

.toggle-btn:hover {
  background-color: var(--bg-tertiary);
}

.model-select {
  width: 100%;
  height: 36px;
  padding: 0 12px;
  background-color: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 14px;
  cursor: pointer;
}

.model-select:focus {
  outline: none;
  border-color: var(--accent);
}

.model-select option {
  background-color: var(--bg-secondary);
  color: var(--text-primary);
}

.panel-footer {
  display: flex;
  justify-content: flex-end;
  padding: 16px 20px;
  border-top: 1px solid var(--border-color);
}

.save-btn {
  width: 80px;
  height: 36px;
  border: none;
  background-color: var(--accent);
  color: white;
  border-radius: 4px;
  font-size: 14px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.save-btn:hover:not(:disabled) {
  background-color: var(--accent-hover);
}

.save-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.hint {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}
</style>