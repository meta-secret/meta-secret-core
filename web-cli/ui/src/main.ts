import {createApp} from "vue";
import {createPinia} from "pinia";

import AppManager from "./AppManager.vue";
import router from "./router";

import "./index.css"

const pinia = createPinia()
const app = createApp(AppManager);

app.use(pinia);
app.use(router);

app.mount("#app");
