<template>
  <div class="ask-panel">
    <div class="dialog-header">
      <div class="question-tabs">
        <button
          v-for="(q, index) in normalizedQuestions"
          :key="q.__key"
          class="tab"
          :class="{ active: currentIndex === index }"
          @click="currentIndex = index"
        >
          {{ index + 1 }}
        </button>
      </div>
      <button class="close-btn" @click="onCancel" title="取消">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <path d="M1 1l12 12M13 1L1 13" />
        </svg>
      </button>
    </div>

    <div class="dialog-body">
      <div class="question-content" v-if="currentQuestion">
        <div class="question-text">{{ currentQuestion.question || '(空问题)' }}</div>

        <div class="options" v-if="currentQuestion.options && currentQuestion.options.length > 0">
          <label
            v-for="opt in currentQuestion.options"
            :key="opt.__key || opt.id"
            class="option"
            :class="{ selected: isSelected(opt.id) }"
            @click="toggleOption(opt.id)"
          >
            <span class="radio">{{ isSelected(opt.id) ? (currentQuestion?.multi_select ? '☑' : '●') : (currentQuestion?.multi_select ? '☐' : '○') }}</span>
            <span class="option-text">{{ opt.text || '(空选项)' }}</span>
          </label>
        </div>

        <div class="free-input">
          <input
            type="text"
            v-model="freeTexts[currentQuestion.__key]"
            placeholder="其他意见..."
          />
        </div>
      </div>
      <div v-else class="empty-state">
        暂无问题
      </div>
    </div>

    <div class="dialog-footer">
      <div class="footer-left">
        <span class="step-hint">{{ currentIndex + 1 }} / {{ normalizedQuestions.length }}</span>
      </div>
      <div class="footer-right">
        <button class="cancel-btn" @click="onCancel">取消</button>
        <button class="submit-btn" @click="onSubmit">提交</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import type { AskQuestion, AskOption } from '../types/api'

type NormalizedOption = AskOption & { __key: string }
type NormalizedQuestion = Omit<AskQuestion, 'options'> & { options: NormalizedOption[]; __key: string }

const props = defineProps<{
  questions: AskQuestion[]
}>()

const emit = defineEmits<{
  (e: 'submit', answers: { questionId: string; selected: string[]; freeText: string }[]): void
  (e: 'cancel'): void
}>()

const currentIndex = ref(0)
const selectedOptions = ref<Record<string, string[]>>({})
const freeTexts = ref<Record<string, string>>({})

// 归一化：确保 id、options 数组、option.id 都存在；用 __key 兜底做 :key
// 容忍模型漏字段（id 缺失、options 不是数组等）导致的渲染问题
const normalizedQuestions = computed<NormalizedQuestion[]>(() => {
  if (!Array.isArray(props.questions)) return []
  return props.questions
    .filter((q): q is AskQuestion => !!q && typeof q === 'object')
    .map((q, qi) => {
      const qKey = q.id ?? `__q_${qi}`
      const opts: NormalizedOption[] = Array.isArray(q.options)
        ? q.options
            .filter((o): o is AskOption => !!o && typeof o === 'object')
            .map((o, oi) => ({ id: o.id ?? `__opt_${qi}_${oi}`, text: o.text ?? '', __key: o.id ?? `__opt_${qi}_${oi}` }))
        : []
      // 兼容 camelCase
      const multi = (q as any).multi_select ?? (q as any).multiSelect ?? false
      return {
        id: qKey,
        question: q.question ?? '',
        options: opts,
        multi_select: !!multi,
        __key: qKey,
      }
    })
})

const currentQuestion = computed(() => normalizedQuestions.value[currentIndex.value])

watch(normalizedQuestions, (qs) => {
  // 把 currentIndex 拉回到有效范围（questions 数量变化时）
  if (currentIndex.value >= qs.length) {
    currentIndex.value = 0
  }
  const sel: Record<string, string[]> = {}
  const texts: Record<string, string> = {}
  qs.forEach(q => { sel[q.__key] = []; texts[q.__key] = '' })
  selectedOptions.value = sel
  freeTexts.value = texts
}, { immediate: true })

function isSelected(optId: string): boolean {
  const q = currentQuestion.value
  if (!q) return false
  return selectedOptions.value[q.__key]?.includes(optId) || false
}

function toggleOption(optId: string) {
  const q = currentQuestion.value
  if (!q) return
  if (!selectedOptions.value[q.__key]) {
    selectedOptions.value[q.__key] = []
  }
  const arr = selectedOptions.value[q.__key]
  const idx = arr.indexOf(optId)
  if (idx >= 0) {
    arr.splice(idx, 1)
  } else {
    if (q.multi_select) {
      arr.push(optId)
    } else {
      arr.splice(0, arr.length, optId)
    }
  }
}

function onSubmit() {
  const answers = normalizedQuestions.value.map(q => ({
    questionId: q.id,
    selected: selectedOptions.value[q.__key] || [],
    freeText: freeTexts.value[q.__key] || ''
  }))
  emit('submit', answers)
}

function onCancel() {
  emit('cancel')
}
</script>

<style scoped>
.ask-panel {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  margin: 0 12px;
  overflow: hidden;
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
}

.question-tabs {
  display: flex;
  gap: 4px;
}

.tab {
  width: 24px;
  height: 24px;
  border: none;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.tab.active {
  background: var(--accent);
  color: white;
}

.close-btn {
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 14px;
  cursor: pointer;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.dialog-body {
  padding: 12px 14px;
}

.question-text {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 10px;
}

.empty-state {
  padding: 24px 0;
  text-align: center;
  color: var(--text-secondary);
  font-size: 13px;
}

.options {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 10px;
}

.option {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  background: var(--bg-tertiary);
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s;
}

.option:hover {
  background: var(--bg-input);
}

.option.selected {
  outline: 1px solid var(--accent);
}

.radio {
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.option.selected .radio {
  color: var(--accent);
}

.option-text {
  font-size: 13px;
  color: var(--text-primary);
}

.free-input {
  margin-top: 4px;
}

.free-input input {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  background: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
}

.free-input input:focus {
  border-color: var(--accent);
}

.dialog-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-top: 1px solid var(--border-color);
}

.step-hint {
  font-size: 12px;
  color: var(--text-secondary);
}

.footer-right {
  display: flex;
  gap: 6px;
}

.cancel-btn {
  padding: 5px 14px;
  border: none;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}

.cancel-btn:hover {
  background: var(--bg-input);
  color: var(--text-primary);
}

.submit-btn {
  padding: 5px 16px;
  border: none;
  background: var(--btn-run);
  color: white;
  border-radius: 4px;
  font-size: 12px;
  cursor: pointer;
}

.submit-btn:hover {
  background: var(--btn-run-hover);
}
</style>
