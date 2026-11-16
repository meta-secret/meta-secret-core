import { createApp } from 'vue';
import Fixture from './LiveboardFixture.vue';
import { createRouter, createWebHistory } from 'vue-router';
const router = createRouter({
  history: createWebHistory(),
  routes: [{ path: '/:pathMatch(.*)*', component: { render: () => null } }],
});
createApp(Fixture).use(router).mount('#app');
