import { createRouter, createWebHistory } from 'vue-router'

const AgentView = () => import('../views/AgentView.vue')

const routes = [
  { path: '/ace', name: 'ace', component: AgentView, props: { agentType: 'ace' } },
  { path: '/front', name: 'front', component: AgentView, props: { agentType: 'front' } },
  { path: '/plan', name: 'plan', component: AgentView, props: { agentType: 'plan' } },
  { path: '/work', name: 'work', component: AgentView, props: { agentType: 'work' } },
  { path: '/review', name: 'review', component: AgentView, props: { agentType: 'review' } },
  { path: '/explore', name: 'explore', component: AgentView, props: { agentType: 'explore' } },
  { path: '/:pathMatch(.*)*', redirect: () => {
    return localStorage.getItem('lastRoute') || '/ace'
  }},
]

const router = createRouter({
  history: createWebHistory(),
  routes,
})

router.afterEach((to) => {
  localStorage.setItem('lastRoute', to.path)
})

export default router