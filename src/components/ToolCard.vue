<template>
  <div class="tool-card" :class="`tool-card-${toolKind}`">
    <div class="tool-card-head">
      <span class="tool-card-icon">{{ icon }}</span>
      <span class="tool-card-name">{{ toolName }}</span>
      <code v-if="filePath" class="tool-card-path">{{ filePath }}</code>
      <span v-if="language" class="tool-card-lang">{{ language }}</span>
    </div>

    <!-- Diff view for edit_file (expanded by default) -->
    <div v-if="toolKind === 'edit'" class="tool-card-diff">
      <div class="diff-header">
        <span class="diff-marker diff-del">−</span>
        <span class="diff-label">修改前</span>
      </div>
      <div class="diff-content diff-del-content" v-html="diffOld"></div>
      <div class="diff-header">
        <span class="diff-marker diff-ins">+</span>
        <span class="diff-label">修改后</span>
      </div>
      <div class="diff-content diff-ins-content" v-html="diffNew"></div>
    </div>

    <!-- Code block for read_file (collapsible, default collapsed) -->
    <div v-else-if="toolKind === 'code' && isReadTool" class="tool-card-code">
      <div class="tool-card-toggle" @click="codeCollapsed = !codeCollapsed">
        <span class="toggle-arrow" :class="{ collapsed: codeCollapsed }"></span>
        <span class="toggle-label">读取结果</span>
      </div>
      <div v-show="!codeCollapsed" v-html="codeHtml"></div>
    </div>

    <!-- Code block for write_file (no collapse, as-is) -->
    <div v-else-if="toolKind === 'code'" class="tool-card-code" v-html="codeHtml"></div>

    <!-- Terminal style for run_command / run_background (streaming) -->
    <div v-else-if="toolKind === 'run'" class="tool-card-run">
      <pre v-if="command" class="tool-card-cmd"><span class="prompt">$</span> {{ command }}</pre>
      <div class="tool-card-run-head">
        <span class="run-status" :class="`run-status-${runState.state}`">{{ runStatusLabel }}</span>
        <button
          v-if="canCancel && runState.call_id"
          class="run-cancel"
          @click="onCancel"
          :disabled="cancelling"
          :title="`取消 call_id=${runState.call_id}`"
        >{{ cancelling ? '取消中…' : '取消' }}</button>
      </div>
      <template v-if="runOutput">
        <div class="tool-card-toggle" @click="runCollapsed = !runCollapsed">
          <span class="toggle-arrow" :class="{ collapsed: runCollapsed }"></span>
          <span class="toggle-label">执行输出</span>
        </div>
        <pre v-show="!runCollapsed" class="tool-card-output">{{ runOutput }}</pre>
      </template>
    </div>

    <!-- Compact output for list/delete/exists operations -->
    <div v-else-if="toolKind === 'compact'" class="tool-card-compact">
      <template v-if="result">
        <div class="tool-card-toggle" @click="compactCollapsed = !compactCollapsed">
          <span class="toggle-arrow" :class="{ collapsed: compactCollapsed }"></span>
          <span class="toggle-label">结果</span>
        </div>
        <pre v-show="!compactCollapsed" class="tool-card-output">{{ result }}</pre>
      </template>
    </div>

    <!-- Default fallback (MCP tools: web_search, understand_image, etc.) -->
    <div v-else>
      <details v-if="parsedArgs" class="tool-card-args" open>
        <summary>参数</summary>
        <pre>{{ argsJson }}</pre>
      </details>
      <template v-if="resultText">
        <div class="tool-card-toggle" @click="defaultCollapsed = !defaultCollapsed">
          <span class="toggle-arrow" :class="{ collapsed: defaultCollapsed }"></span>
          <span class="toggle-label">结果</span>
        </div>
        <pre v-show="!defaultCollapsed" class="tool-card-output">{{ resultText }}</pre>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, type ComputedRef } from 'vue'
import { renderHighlightedBlock, renderSearchReplaceParts, langFromPath } from '../composables/useMarkdown'
import { useCommandStream, type CommandStreamState } from '../composables/useCommandStream'

interface ToolInfo {
  name: string
  args?: string
  result?: string
  /** tool_use_id from the LLM — used to key into the streaming command store. */
  tool_id?: string
  /** Legacy: tool_calls JSON blob (parsed for tool_use_id as fallback). */
  tool_calls?: string
}

const props = defineProps<{
  toolInfo: ToolInfo
}>()

const codeCollapsed = ref(true)
const runCollapsed = ref(true)
const compactCollapsed = ref(true)
const defaultCollapsed = ref(true)
const cancelling = ref(false)

const { getCommandStream, abortCommand } = useCommandStream()

const parsedArgs = computed(() => {
  if (!props.toolInfo.args) return null
  try {
    return JSON.parse(props.toolInfo.args)
  } catch {
    return null
  }
})

const argsJson = computed(() => {
  if (!parsedArgs.value) return props.toolInfo.args || ''
  return JSON.stringify(parsedArgs.value, null, 2)
})

const toolName = computed(() => {
  const name = props.toolInfo.name || 'tool'
  return name.replace(/^(mcp_|filesystem_)/, '')
})

const isReadTool = computed(() => {
  const n = toolName.value.toLowerCase()
  return n.includes('read') && !n.includes('write') && !n.includes('edit')
})

const toolKind = computed(() => {
  const name = toolName.value.toLowerCase()
  if (name.includes('edit')) return 'edit'
  if (name.includes('write')) return 'code'
  if (name.includes('read')) return 'code'
  if (name.includes('run_command') || name.includes('run_background')) return 'run'
  if (name.includes('list') || name.includes('delete') || name.includes('exists')) return 'compact'
  return 'default'
})

const icon = computed(() => {
  switch (toolKind.value) {
    case 'edit': return '✎'
    case 'code': return '▤'
    case 'run': return '⚡'
    case 'compact': return '▣'
    default: return '▣'
  }
})

const filePath = computed(() => {
  if (!parsedArgs.value) return null
  return parsedArgs.value.path
    || parsedArgs.value.file_path
    || parsedArgs.value.filename
    || null
})

const language = computed(() => {
  const path = filePath.value || ''
  const ext = path.split('.').pop()?.toLowerCase() || ''
  return langFromPath(ext)
})

const command = computed(() => {
  if (!parsedArgs.value) return null
  return parsedArgs.value.command || parsedArgs.value.cmd || null
})

const resultText = computed(() => {
  return props.toolInfo.result || ''
})

const result = computed(() => {
  return props.toolInfo.result || ''
})

// Extract tool_use_id from either the explicit prop or the legacy
// tool_calls JSON blob. Used to look up the live streaming state.
const toolUseId = computed<string | null>(() => {
  if (props.toolInfo.tool_id) return props.toolInfo.tool_id
  if (!props.toolInfo.tool_calls) return null
  try {
    const arr = JSON.parse(props.toolInfo.tool_calls)
    if (Array.isArray(arr) && arr.length > 0 && arr[0]?.id) return String(arr[0].id)
  } catch {
    // fall through
  }
  return null
})

// Live streaming state for this tool (null when not a command or no events yet).
const liveStream = computed<ComputedRef<CommandStreamState> | null>(() => {
  if (toolKind.value !== 'run') return null
  const id = toolUseId.value
  if (!id) return null
  return getCommandStream(id)
})

// Output shown in the terminal: prefer live stream (when available and has any
// data) over the static `result` from the persisted tool card. This makes the
// ToolCard transition smoothly: streaming during run, then a stable final
// string after tool_end.
const runOutput = computed(() => {
  const live = liveStream.value?.value
  if (live && (live.output || live.state !== 'end')) return live.output
  return props.toolInfo.result || ''
})

const runState = computed<CommandStreamState>(() => {
  return liveStream.value?.value ?? {
    call_id: '',
    tool_id: toolUseId.value ?? '',
    command: '',
    output: '',
    stdout: '',
    stderr: '',
    state: 'end',
    exit_code: null,
    killed: false,
    truncated: false,
    orphan: false,
  }
})

const runStatusLabel = computed(() => {
  const s = runState.value
  if (s.state === 'begin' || s.state === 'streaming') return '执行中…'
  if (s.state === 'error') return `错误: ${s.error || '未知'}`
  if (s.killed) return '已取消'
  if (s.truncated) return '已完成 (输出截断)'
  if (s.exit_code === 0) return '退出 0'
  if (s.exit_code != null) return `退出 ${s.exit_code}`
  return '已完成'
})

const canCancel = computed(() => {
  const s = runState.value
  return (s.state === 'streaming' || s.state === 'begin') && !!s.call_id
})

async function onCancel() {
  const callId = runState.value.call_id
  if (!callId || cancelling.value) return
  cancelling.value = true
  try {
    await abortCommand(callId)
  } finally {
    cancelling.value = false
  }
}

const codeHtml = computed(() => {
  if (toolKind.value !== 'code') return ''
  return renderHighlightedBlock(props.toolInfo.result || '', language.value || 'plaintext')
})

const diffOld = computed(() => {
  if (toolKind.value !== 'edit' || !parsedArgs.value) return ''
  const search = parsedArgs.value.search
  // Allow empty search (for inserts) - show as empty
  const { oldHtml } = renderSearchReplaceParts(search || '', parsedArgs.value.replace || '', filePath.value || '')
  return oldHtml
})

const diffNew = computed(() => {
  if (toolKind.value !== 'edit' || !parsedArgs.value) return ''
  const replace = parsedArgs.value.replace
  // Allow empty replace (for deletes) - show as empty
  const { newHtml } = renderSearchReplaceParts(parsedArgs.value.search || '', replace || '', filePath.value || '')
  return newHtml
})
</script>

<style scoped>
.tool-card {
  background: var(--bg-secondary);
  border: 1px solid var(--accent-warn);
  border-radius: 8px;
  padding: 10px 14px;
  margin: 4px 0;
  font-family: var(--font-mono);
  font-size: 12px;
}

.tool-card-edit {
  border-color: var(--accent-ok);
}

.tool-card-run {
  border-color: var(--accent-info);
}

.tool-card-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
  flex-wrap: wrap;
}

.tool-card-icon {
  font-size: 14px;
  color: var(--accent-warn);
}

.tool-card-name {
  font-weight: 500;
  color: var(--accent);
}

.tool-card-path {
  color: var(--text-brand);
  background: var(--bg-tertiary);
  padding: 1px 8px;
  border-radius: 4px;
  font-size: 11px;
}

.tool-card-lang {
  color: var(--text-secondary);
  background: var(--bg-tertiary);
  padding: 1px 8px;
  border-radius: 4px;
  font-size: 11px;
}

/* Collapse toggle */
.tool-card-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  user-select: none;
  padding: 4px 0;
  margin-bottom: 4px;
}

.tool-card-toggle:hover {
  color: var(--text-primary);
}

.toggle-arrow {
  display: inline-block;
  width: 6px;
  height: 6px;
  border-right: 1.5px solid var(--text-secondary);
  border-bottom: 1.5px solid var(--text-secondary);
  transform: rotate(-45deg);
  transition: transform 0.2s;
  flex-shrink: 0;
}

.toggle-arrow.collapsed {
  transform: rotate(45deg);
}

.toggle-label {
  font-size: 11px;
  color: var(--text-secondary);
  font-family: inherit;
}

.tool-card-diff {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.diff-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 0;
}

.diff-marker {
  font-weight: 600;
  width: 16px;
}

.diff-del { color: #ef4444; }
.diff-ins { color: #22c55e; }

.diff-label {
  font-size: 10px;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.diff-content {
  border-radius: 4px;
  padding: 8px 10px;
  overflow-x: auto;
  max-height: 200px;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.5;
}

.diff-del-content {
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.2);
}

.diff-ins-content {
  background: rgba(34, 197, 94, 0.1);
  border: 1px solid rgba(34, 197, 94, 0.2);
}

.tool-card-code {
  border-radius: 4px;
  overflow-x: auto;
  max-height: 300px;
}

.tool-card-code :deep(pre) {
  margin: 0;
  padding: 10px 12px;
  background: var(--bg-tertiary);
  border-radius: 6px;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.5;
}

.tool-card-run {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.tool-card-cmd {
  margin: 0;
  padding: 8px 10px;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--text-primary);
}

.tool-card-cmd .prompt {
  color: var(--text-secondary);
  margin-right: 6px;
}

.tool-card-run-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 0;
  flex-wrap: wrap;
}

.run-status {
  font-size: 11px;
  font-family: inherit;
  padding: 1px 8px;
  border-radius: 4px;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
}
.run-status-begin, .run-status-streaming {
  color: var(--accent-info, #38bdf8);
  background: rgba(56, 189, 248, 0.1);
}
.run-status-error {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.1);
}
.run-status-end {
  color: var(--text-secondary);
}

.run-cancel {
  font-family: inherit;
  font-size: 11px;
  padding: 1px 10px;
  border-radius: 4px;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border: 1px solid var(--border-color, rgba(255,255,255,0.1));
  cursor: pointer;
}
.run-cancel:hover:not(:disabled) {
  color: #ef4444;
  border-color: #ef4444;
}
.run-cancel:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.tool-card-output {
  margin: 0;
  padding: 8px 10px;
  background: var(--bg-tertiary);
  border-radius: 4px;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--text-secondary);
  max-height: 400px;
  overflow-y: auto;
}

.tool-card-compact .tool-card-output {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.tool-card-args {
  margin-bottom: 6px;
}

.tool-card-args summary {
  cursor: pointer;
  color: var(--text-secondary);
  font-size: 11px;
  padding: 2px 0;
}

.tool-card-args summary:hover {
  color: var(--text-brand);
}

.tool-card-args pre {
  margin: 4px 0 0 0;
  padding: 6px 8px;
  background: var(--bg-primary);
  border-radius: 4px;
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 11px;
}
</style>
