import { computed, ref } from "vue";

const storageKey = "docmind.sidebar.collapsed";

const sidebarCollapsed = ref(false);
let initialized = false;

const loadSidebarState = () => {
  if (typeof window === "undefined") {
    return;
  }

  sidebarCollapsed.value = window.localStorage.getItem(storageKey) === "1";
};

const persistSidebarState = (collapsed: boolean) => {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(storageKey, collapsed ? "1" : "0");
};

export const useSidebarState = () => {
  if (!initialized) {
    loadSidebarState();
    initialized = true;
  }

  const toggleSidebar = () => {
    sidebarCollapsed.value = !sidebarCollapsed.value;
    persistSidebarState(sidebarCollapsed.value);
  };

  const setSidebarCollapsed = (collapsed: boolean) => {
    sidebarCollapsed.value = collapsed;
    persistSidebarState(collapsed);
  };

  return {
    sidebarCollapsed,
    sidebarWidth: computed(() => (sidebarCollapsed.value ? 68 : 200)),
    toggleSidebar,
    setSidebarCollapsed,
  };
};
