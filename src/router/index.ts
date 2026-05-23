import { createRouter, createWebHashHistory } from "vue-router";
import AppSearch from "../pages/AppSearch.vue";
import ChunksPage from "../pages/ChunksPage.vue";
import LibraryPage from "../pages/LibraryPage.vue";
import StatusPage from "../pages/StatusPage.vue";
import SettingsPage from "../pages/SettingsPage.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "search", component: AppSearch },
    { path: "/chunks", name: "chunks", component: ChunksPage },
    { path: "/library", name: "library", component: LibraryPage },
    { path: "/status", name: "status", component: StatusPage },
    { path: "/settings", name: "settings", component: SettingsPage },
  ],
});
