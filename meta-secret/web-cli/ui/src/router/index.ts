import { createRouter, createWebHistory } from 'vue-router';
import SplitView from '../views/tools/SplitView.vue';
import RecoverView from '../views/tools/RecoverView.vue';
import VaultView from '../views/VaultView.vue';
import ContactView from '../views/ContactView.vue';
import NotFoundView from '../views/404View.vue';
import DocumentationView from '../views/tools/DocumentationView.vue';

import VaultDevices from '../components/vault/Devices.vue';
import VaultSecrets from '../components/vault/Secrets.vue';

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'home',
      component: VaultView,
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
      path: '/tools/split',
      name: 'split',
      component: SplitView,
    },
    {
      path: '/tools/recover',
      name: 'recover',
      component: RecoverView,
    },
    {
      path: '/tools/docs',
      name: 'documentation',
      component: DocumentationView,
    },
    {
      path: '/contact',
      name: 'contact',
      component: ContactView,
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

export default router;
