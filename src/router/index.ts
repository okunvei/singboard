import { createRouter, createWebHistory } from 'vue-router'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      redirect: '/proxies',
    },
    {
      path: '/overview',
      name: 'overview',
      component: () => import('@/views/OverviewPage.vue'),
    },
    {
      path: '/proxies',
      name: 'proxies',
      component: () => import('@/views/ProxiesPage.vue'),
    },
    {
      path: '/connections',
      name: 'connections',
      component: () => import('@/views/ConnectionsPage.vue'),
    },
    {
      path: '/logs',
      name: 'logs',
      component: () => import('@/views/LogsPage.vue'),
    },
    {
      path: '/rules',
      name: 'rules',
      component: () => import('@/views/RulesPage.vue'),
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('@/views/SettingsPage.vue'),
    },
    {
      path: '/config-editor',
      name: 'config-editor',
      component: () => import('@/views/ConfigEditorPage.vue'),
    },
  ],
})

export default router
