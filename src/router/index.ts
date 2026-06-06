import { createRouter, createWebHistory } from 'vue-router'

const AgentView = () => import('../views/AgentView.vue')

const routes = [
  { path: '/ace', name: 'ace', component: AgentView, props: { agentType: 'ace' } },
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