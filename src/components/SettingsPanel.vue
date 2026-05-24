<template>
  <div class="settings-overlay" v-if="visible" @click.self="close">
    <div class="settings-panel">
      <div class="panel-header">
        <h2>设置</h2>
        <button class="close-btn" @click="close">✕</button>
      </div>
      <div class="panel-content" v-if="loaded">
        <!-- Provider Tabs -->
        <div class="provider-tabs">
          <button :class="['provider-tab', { active: provider === 'minimax' }]" @click="provider = 'minimax'">MiniMax</button>
          <button :class="['provider-tab', { active: provider === 'custom' }]" @click="provider = 'custom'">自定义 Anthropic</button>
          <button :class="['provider-tab', { active: provider === 'agents' }]" @click="provider = 'agents'">Team 配置</button>
        </div>

        <!-- MiniMax Panel -->
        <div v-if="provider === 'minimax'" class="provider-panel">
          <div class="form-group">
            <label>API Key</label>
            <div class="input-wrapper">
              <input :type="showKey ? 'text' : 'password'" v-model="minimaxApiKey" placeholder="输入 MiniMax API Key..." @focus="handleKeyFocus" />
              <button class="toggle-btn" @click="showKey = !showKey">{{ showKey ? '👁' : '👁‍🗨' }}</button>
            </div>
          </div>
          <div class="form-group">
            <label>模型</label>
            <select v-model="model" class="model-select">
              <option value="MiniMax-M2.7">MiniMax-M2.7 (204,800)</option>
              <option value="MiniMax-M2.7-highspeed">MiniMax-M2.7-highspeed (204,800)</option>
              <option value="MiniMax-M2.5">MiniMax-M2.5 (204,800)</option>
              <option value="MiniMax-M2.5-highspeed">MiniMax-M2.5-highspeed (204,800)</option>
              <option value="MiniMax-M2.1">MiniMax-M2.1 (204,800)</option>
              <option value="MiniMax-M2.1-highspeed">MiniMax-M2.1-highspeed (204,800)</option>
              <option value="MiniMax-M2">MiniMax-M2 (204,800)</option>
            </select>
          </div>
        </div>

        <!-- Agent Models Panel -->
        <div v-if="provider === 'agents'" class="provider-panel">
          <p class="hint" style="margin-bottom:12px">为每个 Team 智能体单独指定模型。留空则使用上方配置的全局模型。</p>
          <div v-for="ag in teamAgents" :key="ag.key" class="agent-config-row">
            <span class="agent-label">{{ ag.name }}</span>
            <select v-model="agentModels[ag.key]" class="agent-model-select">
              <option value="">使用全局配置</option>
              <optgroup label="MiniMax">
                <option value="minimax|MiniMax-M2.7|204800">MiniMax-M2.7</option>
                <option value="minimax|MiniMax-M2.7-highspeed|204800">M2.7-highspeed</option>
                <option value="minimax|MiniMax-M2.5|204800">MiniMax-M2.5</option>
                <option value="minimax|MiniMax-M2.5-highspeed|204800">M2.5-highspeed</option>
                <option value="minimax|MiniMax-M2.1|204800">MiniMax-M2.1</option>
                <option value="minimax|MiniMax-M2.1-highspeed|204800">M2.1-highspeed</option>
                <option value="minimax|MiniMax-M2|204800">MiniMax-M2</option>
              </optgroup>
              <optgroup v-if="customConfigs.length > 0" label="自定义模型">
                <option v-for="cfg in customConfigs" :key="cfg.id" :value="`custom|${cfg.model}|${cfg.context_window}`">{{ cfg.name }} ({{ cfg.model }})</option>
              </optgroup>
            </select>
          </div>
        </div>

        <!-- Custom Anthropic Panel -->
        <div v-if="provider === 'custom'" class="provider-panel">
          <!-- Saved configs list -->
          <div v-if="customConfigs.length > 0" class="config-list">
            <div
              v-for="cfg in customConfigs" :key="cfg.id"
              class="config-item"
              :class="{ active: activeConfigId === cfg.id }"
              @click="selectConfig(cfg)"
            >
              <div class="config-info">
                <span class="config-name">{{ cfg.name }}</span>
                <span class="config-model">{{ cfg.model }} · {{ formatCtx(cfg.context_window) }}</span>
              </div>
              <button class="config-del" @click.stop="removeConfig(cfg.id)" title="删除">×</button>
            </div>
          </div>

          <!-- Save current as config -->
          <div class="config-save-row">
            <input
              v-model="newConfigName"
              class="config-name-input"
              placeholder="配置名称，如 Claude Opus"
            />
            <button class="config-save-btn" @click="addConfig" :disabled="!newConfigName.trim()">+ 保存当前配置</button>
          </div>

          <div class="form-group">
            <label>API 地址</label>
            <input type="text" v-model="customApiUrl" placeholder="https://api.anthropic.com" />
            <p class="hint">支持任何 Anthropic Messages API 兼容的服务</p>
          </div>
          <div class="form-group">
            <label>API Key</label>
            <div class="input-wrapper">
              <input :type="showCustomKey ? 'text' : 'password'" v-model="customApiKey" placeholder="sk-ant-..." @focus="handleCustomKeyFocus" />
              <button class="toggle-btn" @click="showCustomKey = !showCustomKey">{{ showCustomKey ? '👁' : '👁‍🗨' }}</button>
            </div>
          </div>
          <div class="form-group">
            <label>模型名称</label>
            <input type="text" v-model="customModel" placeholder="claude-sonnet-4-6" />
            <p class="hint">输入完整模型 ID，如 claude-opus-4-7、claude-sonnet-4-6</p>
          </div>
          <div class="form-group">
            <label>上下文窗口 (tokens)</label>
            <input type="number" v-model.number="customContextWindow" step="1000" min="8000" max="1000000" />
            <p class="hint">须与模型真实上下文窗口一致，超出部分不会生效</p>
          </div>
        </div>

        <div class="form-group">
          <label>权限模式</label>
          <select v-model="permMode" class="model-select">
            <option value="full">Full — 全部自动放行</option>
            <option value="normal">Normal — 安全命令自动，危险确认</option>
            <option value="guarded">Guarded — 修改操作都需确认</option>
          </select>
        </div>

      </div>
      <div class="panel-footer">
        <button class="save-btn" @click="saveAll" :disabled="saving">
          {{ saving ? '保存中...' : '保存并使用' }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'close'): void }>()

interface AgentModelConfig {
  provider: string
  model: string
  context_window: number
}

interface ProviderConfig {
  provider: string
  minimax_api_key: string
  model: string
  custom_api_url: string
  custom_api_key: string
  custom_model: string
  context_window: number
  custom_context_window: number
}

const loaded = ref(false)
const provider = ref('minimax')
const minimaxApiKey = ref('')
const model = ref('MiniMax-M2.7')
const customApiUrl = ref('')
const customApiKey = ref('')
const customModel = ref('')
const contextWindow = ref(204800)
const customContextWindow = ref(200000)
const permMode = ref('normal')
const saving = ref(false)
const showKey = ref(false)
const showCustomKey = ref(false)

function handleKeyFocus() {
  if (minimaxApiKey.value.includes('****')) {
    minimaxApiKey.value = ''
  }
}
function handleCustomKeyFocus() {
  if (customApiKey.value.includes('****')) {
    customApiKey.value = ''
  }
}

// Per-agent model config
const teamAgents = [
  { key: 'front', name: 'Front — 入口' },
  { key: 'plan', name: 'Plan — 方案' },
  { key: 'work', name: 'Work — 执行' },
  { key: 'review', name: 'Review — 审查' },
  { key: 'explore', name: 'Explore — 探索' },
]
const agentModels = ref<Record<string, string>>({})

async function loadAgentConfigs() {
  for (const ag of teamAgents) {
    try {
      const cfg = await invoke<AgentModelConfig | null>('get_agent_model_config', { agentType: ag.key })
      if (cfg) {
        agentModels.value[ag.key] = `${cfg.provider}|${cfg.model}|${cfg.context_window}`
      } else {
        agentModels.value[ag.key] = ''
      }
    } catch { agentModels.value[ag.key] = '' }
  }
}

// Custom configs list
interface CustomConfig { id: number; name: string; api_url: string; api_key: string; model: string; context_window: number }
const customConfigs = ref<CustomConfig[]>([])
const newConfigName = ref('')
const activeConfigId = ref<number | null>(null)

async function loadCustomConfigs() {
  try {
    customConfigs.value = await invoke<CustomConfig[]>('list_custom_configs')
  } catch { customConfigs.value = [] }
}

async function addConfig() {
  const name = newConfigName.value.trim()
  if (!name || !customApiUrl.value.trim() || !customModel.value.trim()) return
  try {
    await invoke('save_custom_config', {
      name, apiUrl: customApiUrl.value, apiKey: customApiKey.value, model: customModel.value, contextWindow: customContextWindow.value
    })
    newConfigName.value = ''
    await loadCustomConfigs()
  } catch (e) { console.error('Failed to save config:', e) }
}

function selectConfig(cfg: CustomConfig) {
  customApiUrl.value = cfg.api_url
  customApiKey.value = cfg.api_key
  customModel.value = cfg.model
  customContextWindow.value = cfg.context_window || 200000
  activeConfigId.value = cfg.id
}

function formatCtx(tokens: number): string {
  if (!tokens) return ''
  if (tokens >= 1000000) return `${(tokens / 1000000).toFixed(1)}M`
  if (tokens >= 1000) return `${(tokens / 1000).toFixed(0)}K`
  return `${tokens}`
}

async function removeConfig(id: number) {
  try {
    await invoke('delete_custom_config', { id })
    if (activeConfigId.value === id) activeConfigId.value = null
    await loadCustomConfigs()
  } catch (e) { console.error('Failed to delete config:', e) }
}

watch(() => props.visible, async (val) => {
  if (val) {
    loaded.value = false
    await loadSettings()
    loaded.value = true
  }
})

const loadSettings = async () => {
  try {
    const config = await invoke<ProviderConfig>('get_provider_config')
    provider.value = config.provider || 'minimax'
    minimaxApiKey.value = config.minimax_api_key || ''
    model.value = config.model || 'MiniMax-M2.7'
    customApiUrl.value = config.custom_api_url || ''
    customApiKey.value = config.custom_api_key || ''
    customModel.value = config.custom_model || ''
    contextWindow.value = config.context_window || 204800
    customContextWindow.value = config.custom_context_window || 200000

    const mode = await invoke<string>('get_permission_mode')
    permMode.value = JSON.parse(mode) || 'normal'

    await loadCustomConfigs()
    await loadAgentConfigs()
  } catch (e) {
    console.error('Failed to load settings:', e)
  }
}

const saveAll = async () => {
  saving.value = true
  try {
    await invoke('set_provider_config', {
      config: {
        provider: provider.value,
        minimax_api_key: minimaxApiKey.value,
        model: model.value,
        api_url: 'https://api.minimaxi.com',
        context_window: contextWindow.value,
        custom_api_url: customApiUrl.value.trim(),
        custom_api_key: customApiKey.value.trim(),
        custom_model: customModel.value.trim(),
        custom_context_window: customContextWindow.value,
      }
    })
    await invoke('set_permission_mode', { mode: permMode.value })
    // Save per-agent model configs
    for (const ag of teamAgents) {
      const val = agentModels.value[ag.key]
      if (val) {
        const [provider, model, cw] = val.split('|')
        await invoke('set_agent_model_config', { agentType: ag.key, provider, model, contextWindow: parseInt(cw) || 204800 })
      } else {
        await invoke('delete_agent_model_config', { agentType: ag.key }).catch(() => {})
      }
    }
    close()
  } catch (e) {
    console.error('Failed to save settings:', e)
  } finally {
    saving.value = false
  }
}

const close = () => emit('close')
</script>

<style scoped>
.settings-overlay {
  position: fixed; inset: 0; z-index: 100;
  display: flex; align-items: center; justify-content: center;
  background: rgba(0,0,0,0.5);
}
.settings-panel {
  width: 440px; max-height: 85vh; overflow-y: auto;
  background: var(--bg-secondary); border: 1px solid var(--border-color);
  border-radius: 12px;
}
.panel-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 20px; border-bottom: 1px solid var(--border-color);
}
.panel-header h2 { font-size: 16px; color: var(--text-primary); margin: 0; }
.close-btn {
  width: 28px; height: 28px; border: none; background: transparent;
  color: var(--text-secondary); font-size: 16px; cursor: pointer; border-radius: 4px;
}
.close-btn:hover { background: var(--bg-tertiary); color: var(--text-primary); }
.panel-content { padding: 16px 20px; }

.provider-tabs {
  display: flex; gap: 0; margin-bottom: 16px;
  background: var(--bg-tertiary); border-radius: 6px; padding: 2px;
}
.provider-tab {
  flex: 1; padding: 8px 0; border: none; background: transparent;
  color: var(--text-secondary); font-size: 13px; cursor: pointer; border-radius: 5px;
  transition: all 0.15s;
}
.provider-tab.active { background: var(--accent); color: white; font-weight: 600; }
.provider-panel { margin-bottom: 12px; }

.config-list {
  max-height: 140px; overflow-y: auto;
  margin-bottom: 12px;
  border: 1px solid var(--border-color); border-radius: 6px;
}
.config-item {
  display: flex; align-items: center; justify-content: space-between;
  padding: 8px 10px; cursor: pointer;
  border-bottom: 1px solid var(--border-color);
  transition: background 0.1s;
}
.config-item:last-child { border-bottom: none; }
.config-item:hover { background: var(--bg-tertiary); }
.config-item.active {
  background: rgba(0, 47, 167, 0.12);
  border-left: 2px solid var(--accent);
}
.config-info { display: flex; flex-direction: column; gap: 1px; overflow: hidden; }
.config-name { font-size: 12px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.config-model { font-size: 11px; color: var(--text-secondary); }
.config-del {
  width: 22px; height: 22px; border: none; background: transparent;
  color: var(--text-secondary); font-size: 14px; cursor: pointer; border-radius: 4px;
  flex-shrink: 0;
}
.config-del:hover { background: var(--bg-input); color: #e81123; }

.config-save-row {
  display: flex; gap: 6px; margin-bottom: 14px;
}
.config-name-input {
  flex: 1; height: 32px; padding: 0 8px;
  background: var(--bg-input); border: 1px solid var(--border-color);
  border-radius: 4px; color: var(--text-primary); font-size: 12px; outline: none;
  box-sizing: border-box;
}
.config-name-input:focus { border-color: var(--accent); }
.config-save-btn {
  padding: 5px 12px; border: 1px solid var(--accent); background: transparent;
  color: var(--accent); border-radius: 4px; font-size: 12px; cursor: pointer; white-space: nowrap;
}
.config-save-btn:hover { background: var(--accent); color: white; }
.config-save-btn:disabled { opacity: 0.4; cursor: default; }

.form-group { margin-bottom: 14px; }
.form-group label { display: block; font-size: 13px; font-weight: 500; color: var(--text-primary); margin-bottom: 5px; }
.form-group input, .form-group select {
  width: 100%; height: 36px; padding: 0 10px;
  background: var(--bg-input); border: 1px solid var(--border-color);
  border-radius: 4px; color: var(--text-primary); font-size: 13px; outline: none;
  box-sizing: border-box;
}
.form-group input:focus, .form-group select:focus { border-color: var(--accent); }
.input-wrapper { display: flex; gap: 4px; }
.input-wrapper input { flex: 1; }
.toggle-btn {
  width: 40px; border: 1px solid var(--border-color); background: var(--bg-tertiary);
  color: var(--text-secondary); border-radius: 4px; cursor: pointer; font-size: 14px;
}
.toggle-btn:hover { background: var(--bg-input); }
.hint { font-size: 11px; color: var(--text-secondary); margin: 3px 0 0; }

.panel-footer {
  padding: 12px 20px; border-top: 1px solid var(--border-color);
  display: flex; justify-content: flex-end;
}
.save-btn {
  padding: 8px 24px; border: none; background: var(--accent);
  color: white; border-radius: 4px; font-size: 13px; cursor: pointer;
}
.save-btn:hover { opacity: 0.9; }

.agent-config-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.agent-label {
  width: 120px;
  font-size: 12px;
  color: var(--text-primary);
  white-space: nowrap;
  flex-shrink: 0;
}

.agent-model-select {
  flex: 1;
  height: 28px;
  padding: 0 6px;
  background: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 11px;
  outline: none;
  box-sizing: border-box;
}

.agent-model-select:focus {
  border-color: var(--accent);
}
</style>
