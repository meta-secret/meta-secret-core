import { createApp } from 'vue';
import { createPinia } from 'pinia';

import AppManager from './AppManager.vue';
import router from './router';
import createPersistedState from './plugins/persistState';

import './index.css';

const pinia = createPinia();
// Add the persistence plugin to Pinia
pinia.use(createPersistedState());

const app = createApp(AppManager);

app.use(pinia);
app.use(router);

app.mount('#app');
