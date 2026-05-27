import { ref, watch } from "vue";

type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "docmind.theme";

const preferredDark = ref(false);
let mediaQuery: MediaQueryList | null = null;

const themeMode = ref<ThemeMode>("light");

const loadTheme = () => {
  if (typeof window === "undefined") return;
  const stored = window.localStorage.getItem(STORAGE_KEY) as ThemeMode | null;
  themeMode.value = stored ?? "system";
};

const persistTheme = (mode: ThemeMode) => {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(STORAGE_KEY, mode);
};

const resolveTheme = (mode: ThemeMode): "light" | "dark" => {
  if (mode === "system") {
    return preferredDark.value ? "dark" : "light";
  }
  return mode;
};

const applyTheme = (mode: ThemeMode) => {
  const resolved = resolveTheme(mode);
  if (resolved === "dark") {
    document.documentElement.classList.add("dark");
  } else {
    document.documentElement.classList.remove("dark");
  }
  document.documentElement.style.colorScheme = resolved;
};

const setupSystemListener = () => {
  if (typeof window === "undefined") return;
  mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
  preferredDark.value = mediaQuery.matches;

  const handler = (e: MediaQueryListEvent) => {
    preferredDark.value = e.matches;
    if (themeMode.value === "system") {
      applyTheme("system");
    }
  };

  if (mediaQuery.addEventListener) {
    mediaQuery.addEventListener("change", handler);
  }
};

let initialized = false;

export const useTheme = () => {
  if (!initialized) {
    loadTheme();
    setupSystemListener();
    applyTheme(themeMode.value);
    initialized = true;
  }

  const setTheme = (mode: ThemeMode) => {
    themeMode.value = mode;
    persistTheme(mode);
    applyTheme(mode);
  };

  watch(themeMode, (mode) => {
    persistTheme(mode);
    applyTheme(mode);
  });

  const isDark = ref(resolveTheme(themeMode.value) === "dark");

  const updateIsDark = () => {
    isDark.value = resolveTheme(themeMode.value) === "dark";
  };

  watch(themeMode, updateIsDark);

  return {
    themeMode,
    isDark,
    setTheme,
  };
};
