<template>
  <div class="mode-switcher">
    <button
      :class="['mode-btn', { active: mode === 'ace' }]"
      @click="switchTo('ace')"
    >Ace</button>
    <button
      :class="['mode-btn', { active: mode === 'team' }]"
      @click="switchTo('team')"
    >Team</button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'

const router = useRouter()
const route = useRoute()

const mode = computed(() => {
  // Treat root path as ace since default redirect goes to /ace
  if (route.path === '/' || route.path.startsWith('/ace')) return 'ace'
  return 'team'
})

function switchTo(m: 'ace' | 'team') {
  if (m === 'ace') {
    router.push('/ace')
  } else {
    router.push('/front')
  }
}
</script>

<style scoped>
.mode-switcher {
  display: flex;
  gap: 0;
  background: var(--bg-tertiary);
  border-radius: 8px;
  padding: 3px;
  margin: 8px 12px;
}

.mode-btn {
  flex: 1;
  padding: 8px 0;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.mode-btn:hover {
  color: var(--text-primary);
}

.mode-btn.active {
  background: var(--accent);
  color: white;
  font-weight: 600;
}
</style>
