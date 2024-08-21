import {createRouter, createWebHistory} from "vue-router";
import HomeView from "../views/HomeView.vue";
import SplitView from "../views/SplitView.vue";
import RecoverView from "../views/RecoverView.vue";
import VaultView from "../views/VaultView.vue";
import ContactView from "../views/ContactView.vue";
import NotFoundView from "../views/404View.vue";

import VaultDevices from "../components/vault/Devices.vue";
import VaultSecrets from "../components/vault/Secrets.vue";

const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    routes: [
        {
            path: "/",
            name: "home",
            component: HomeView,
        },
        {
            path: "/split",
            name: "split",
            component: SplitView,
        },
        {
            path: "/vault",
            name: "vault",
            component: VaultView,
            children: [
                {
                    path: "",
                    name: 'vault-default',
                    component: VaultSecrets,
                },
                {
                    path: "secrets",
                    name: "vaultSecrets",
                    component: VaultSecrets,
                },
                {
                    path: "devices",
                    name: "vaultDevices",
                    component: VaultDevices,
                },
            ],
        },
        {
            path: "/recover",
            name: "recover",
            component: RecoverView,
        },
        {
            path: "/contact",
            name: "contact",
            component: ContactView,
        },
        {
            path: '/404',
            name: 'PageNotExist',
            component: NotFoundView,
        },
        {
            path: "/:catchAll(.*)",
            redirect: "/404"
        }
    ],
});

export default router;
