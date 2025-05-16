import { createRouter, createWebHistory } from 'vue-router';
import SplitView from '../views/tools/SplitView.vue';
import RecoverView from '../views/tools/RecoverView.vue';
import VaultView from '../views/VaultView.vue';
import ContactView from '../views/ContactView.vue';
import NotFoundView from '../views/404View.vue';
import DocumentationView from '../views/tools/DocumentationView.vue';
import SettingsView from '../views/SettingsView.vue';
import InformationView from '../views/InformationView.vue';

import VaultDevices from '../components/vault/Devices.vue';
import VaultSecrets from '../components/vault/Secrets.vue';
import { useAuthStore } from '@/stores/auth';

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'home',
      component: VaultView,
      meta: { requiresAuth: true },
      children: [
        {
          path: '',
          name: 'home-default',
          component: VaultSecrets,
        },
        {
          path: 'secrets',
          name: 'homeSecrets',
          component: VaultSecrets,
        },
        {
          path: 'devices',
          name: 'homeDevices',
          component: VaultDevices,
        },
      ],
    },
    {
      path: '/info',
      name: 'information',
      component: InformationView,
      meta: { requiresAuth: false },
    },
    {
      path: '/settings',
      name: 'settings',
      component: SettingsView,
      meta: { requiresAuth: true },
    },
    {
      path: '/tools/split',
      name: 'split',
      component: SplitView,
      meta: { requiresAuth: false },
    },
    {
      path: '/tools/recover',
      name: 'recover',
      component: RecoverView,
      meta: { requiresAuth: false },
    },
    {
      path: '/tools/docs',
      name: 'documentation',
      component: DocumentationView,
      meta: { requiresAuth: false },
    },
    {
      path: '/contact',
      name: 'contact',
      component: ContactView,
      meta: { requiresAuth: false },
    },
    {
      path: '/404',
      name: 'PageNotExist',
      component: NotFoundView,
    },
    {
      path: '/:catchAll(.*)',
      redirect: '/404',
    },
  ],
});

// Navigation guard for authentication
router.beforeEach((to, from, next) => {
  const authStore = useAuthStore();
  const requiresAuth = to.matched.some(record => record.meta.requiresAuth);
  
  if (requiresAuth && !authStore.isAuthenticated) {
    // If the route requires authentication and the user is not authenticated,
    // allow navigation but the auth modal will show due to the isAuthenticated state
    next();
  } else {
    // Otherwise proceed normally
    next();
  }
});

export default router;
