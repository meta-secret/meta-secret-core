import { createApp } from 'vue';
import { createMemoryHistory, createRouter } from 'vue-router';
import Fixture from './Fixture.vue';
const router = createRouter({
  history: createMemoryHistory(),
  routes: [{ path: '/:pathMatch(.*)*', component: { render: () => null } }],
});
await router.push(`/${location.search}`);
await router.isReady();
createApp(Fixture).use(router).mount('#app');
