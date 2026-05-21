<template>
  <div class="ask-overlay" @click.self="onCancel">
    <div class="ask-dialog">
      <div class="dialog-header">
        <div class="question-tabs">
          <button
            v-for="(q, index) in questions"
            :key="q.id"
            class="tab"
            :class="{ active: currentIndex === index }"
            @click="currentIndex = index"
          >
            问题 {{ index + 1 }}
          </button>
        </div>
        <button class="close-btn" @click="onCancel">✕</button>
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
            >
              <input
                type="checkbox"
                :checked="isSelected(opt.id)"
                @change="toggleOption(opt.id)"
              />
              <span class="checkbox">{{ isSelected(opt.id) ? '☑' : '☐' }}</span>
              <span class="option-text">{{ opt.text }}</span>
            </label>
          </div>

          <div class="free-input">
            <label>其他意见：</label>
            <input
              type="text"
              v-model="freeTexts[currentQuestion.id]"
              placeholder="输入您的想法..."
            />
          </div>
        </div>
      </div>

      <div class="dialog-footer">
        <button
          class="nav-btn"
          :disabled="currentIndex === 0"
          @click="currentIndex--"
        >
          &lt; 上一个
        </button>
        <button
          class="nav-btn"
          :disabled="currentIndex === questions.length - 1"
          @click="currentIndex++"
        >
          下一个 &gt;
        </button>
        <button class="submit-btn" @click="onSubmit">提交回答</button>
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

// Initialize state
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
    arr.push(optId)
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
.ask-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.ask-dialog {
  background: var(--bg-secondary);
  border-radius: 12px;
  width: 500px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-color);
}

.question-tabs {
  display: flex;
  gap: 4px;
}

.tab {
  padding: 6px 12px;
  border: none;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
}

.tab.active {
  background: var(--accent);
  color: white;
}

.close-btn {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 16px;
  cursor: pointer;
  border-radius: 4px;
}

.close-btn:hover {
  background: var(--bg-tertiary);
}

.dialog-body {
  flex: 1;
  padding: 20px 24px;
  overflow-y: auto;
}

.question-text {
  font-size: 16px;
  font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 16px;
}

.options {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-bottom: 20px;
}

.option {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  background: var(--bg-tertiary);
  border-radius: 8px;
  cursor: pointer;
  transition: background 0.15s;
}

.option:hover {
  background: var(--bg-primary);
}

.option.selected {
  background: var(--accent);
  background-opacity: 0.2;
}

.checkbox {
  font-size: 18px;
}

.option-text {
  font-size: 14px;
  color: var(--text-primary);
}

.free-input {
  margin-top: 16px;
}

.free-input label {
  display: block;
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}

.free-input input {
  width: 100%;
  height: 36px;
  padding: 0 12px;
  background: var(--bg-input);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-primary);
  font-size: 14px;
  outline: none;
}

.free-input input:focus {
  border-color: var(--accent);
}

.dialog-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-top: 1px solid var(--border-color);
}

.nav-btn {
  padding: 8px 16px;
  border: none;
  background: var(--bg-tertiary);
  color: var(--text-primary);
  border-radius: 6px;
  font-size: 13px;
  cursor: pointer;
}

.nav-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.nav-btn:hover:not(:disabled) {
  background: var(--bg-primary);
}

.submit-btn {
  padding: 8px 20px;
  border: none;
  background: var(--btn-run);
  color: white;
  border-radius: 6px;
  font-size: 14px;
  cursor: pointer;
}

.submit-btn:hover {
  background: var(--btn-run-hover);
}
</style>