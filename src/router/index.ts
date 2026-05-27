import { createRouter, createWebHashHistory } from "vue-router";
import AppSearch from "../pages/AppSearch.vue";
import ChunksPage from "../pages/ChunksPage.vue";
import StatusPage from "../pages/StatusPage.vue";
import SettingsPage from "../pages/SettingsPage.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "search", component: AppSearch },
    { path: "/chunks", name: "chunks", component: ChunksPage },
    { path: "/library", redirect: "/status" },
    { path: "/status", name: "status", component: StatusPage },
    { path: "/settings", name: "settings", component: SettingsPage },
  ],
});
