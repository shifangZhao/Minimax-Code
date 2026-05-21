<template>
  <div class="ask-panel">
    <div class="dialog-header">
      <div class="question-tabs">
        <button
          v-for="(q, index) in questions"
          :key="q.id"
          class="tab"
          :class="{ active: currentIndex === index }"
          @click="currentIndex = index"
        >
          {{ index + 1 }}
        </button>
      </div>
      <button class="close-btn" @click="onCancel" title="取消">✕</button>
    </div>

    <div class="dialog-body">
      <div class="question-content" v-if="currentQuestion">
        <div class="question-text">{{ currentQuestion.question }}</div>

        <div class="options">
          <label
            v-for="opt in currentQuestion.options"
            :key="opt.id"
            class="option"
            :class="{ selected: isSelected(opt.id) }"
            @click="toggleOption(opt.id)"
          >
            <span class="radio">{{ isSelected(opt.id) ? (currentQuestion?.multi_select ? '☑' : '●') : (currentQuestion?.multi_select ? '☐' : '○') }}</span>
            <span class="option-text">{{ opt.text }}</span>
          </label>
        </div>

        <div class="free-input">
          <input
            type="text"
            v-model="freeTexts[currentQuestion.id]"
            placeholder="其他意见..."
          />
        </div>
      </div>
    </div>

    <div class="dialog-footer">
      <div class="footer-left">
        <span class="step-hint">{{ currentIndex + 1 }} / {{ questions.length }}</span>
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
import type { AskQuestion } from '../types/api'

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

const currentQuestion = computed(() => props.questions[currentIndex.value])

watch(() => props.questions, () => {
  const sel: Record<string, string[]> = {}
  const texts: Record<string, string> = {}
  props.questions.forEach(q => { sel[q.id] = []; texts[q.id] = '' })
  selectedOptions.value = sel
  freeTexts.value = texts
}, { immediate: true })

function isSelected(optId: string): boolean {
  const q = currentQuestion.value
  if (!q) return false
  return selectedOptions.value[q.id]?.includes(optId) || false
}

function toggleOption(optId: string) {
  const q = currentQuestion.value
  if (!q) return
  if (!selectedOptions.value[q.id]) {
    selectedOptions.value[q.id] = []
  }
  const arr = selectedOptions.value[q.id]
  const idx = arr.indexOf(optId)
  if (idx >= 0) {
    arr.splice(idx, 1)
  } else {
    if (q.multi_select) {
      arr.push(optId)
    } else {
      // Single-select: replace
      arr.splice(0, arr.length, optId)
    }
  }
}

function onSubmit() {
  const answers = props.questions.map(q => ({
    questionId: q.id,
    selected: selectedOptions.value[q.id] || [],
    freeText: freeTexts.value[q.id] || ''
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
