<template>
  <div class="tab-bar">
    <router-link
      v-for="tab in tabs"
      :key="tab.path"
      :to="tab.path"
      class="tab"
      :class="{ active: $route.path === tab.path }"
    >
      <span class="tab-name">{{ tab.name }}</span>
    </router-link>
  </div>
</template>

<script setup lang="ts">
const tabs = [
  { name: 'Front', path: '/front' },
  { name: 'Plan', path: '/plan' },
  { name: 'Work', path: '/work' },
  { name: 'Review', path: '/review' },
  { name: 'Explore', path: '/explore' },
]
</script>

<style scoped>
.tab-bar {
  display: flex;
  background-color: var(--bg-tertiary);
  border-bottom: 1px solid var(--border-color);
  padding: 0 8px;
  gap: 4px;
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 16px;
  text-decoration: none;
  color: var(--text-secondary);
  font-size: 13px;
  border-bottom: 2px solid transparent;
  transition: all 0.15s;
}

.tab:hover {
  color: var(--text-primary);
  background-color: var(--bg-secondary);
}

.tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}

</style>