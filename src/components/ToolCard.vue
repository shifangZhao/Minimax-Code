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

    <!-- Terminal style for run_command -->
    <div v-else-if="toolKind === 'run'" class="tool-card-run">
      <pre v-if="command" class="tool-card-cmd"><span class="prompt">$</span> {{ command }}</pre>
      <template v-if="result">
        <div class="tool-card-toggle" @click="runCollapsed = !runCollapsed">
          <span class="toggle-arrow" :class="{ collapsed: runCollapsed }"></span>
          <span class="toggle-label">执行结果</span>
        </div>
        <pre v-show="!runCollapsed" class="tool-card-output">{{ result }}</pre>
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
import { computed, ref } from 'vue'
import { renderHighlightedBlock, renderSearchReplace, langFromPath } from '../composables/useMarkdown'

interface ToolInfo {
  name: string
  args?: string
  result?: string
}

const props = defineProps<{
  toolInfo: ToolInfo
}>()

const codeCollapsed = ref(true)
const runCollapsed = ref(true)
const compactCollapsed = ref(true)
const defaultCollapsed = ref(true)

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

const codeHtml = computed(() => {
  if (toolKind.value !== 'code') return ''
  return renderHighlightedBlock(props.toolInfo.result || '', language.value || 'plaintext')
})

const diffOld = computed(() => {
  if (toolKind.value !== 'edit' || !parsedArgs.value) return ''
  const search = parsedArgs.value.search
  if (!search) return ''
  return renderSearchReplace(search, parsedArgs.value.replace || '', filePath.value || '')
})

const diffNew = computed(() => {
  if (toolKind.value !== 'edit' || !parsedArgs.value) return ''
  const replace = parsedArgs.value.replace
  if (!replace) return ''
  return renderHighlightedBlock(replace, language.value || 'plaintext')
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
