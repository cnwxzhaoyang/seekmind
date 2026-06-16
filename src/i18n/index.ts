/**
 * @author MorningSun
 * @CreatedDate 2026/06/16
 * @Description SeekMind 界面语言初始化与持久化，默认跟随系统语言，支持中文和英文切换。
 */
import { ref } from "vue";
import { createI18n } from "vue-i18n";
import zhCN from "../locales/zh-CN.json";
import en from "../locales/en.json";

const STORAGE_KEY = "seekmind-locale";
export type LocaleMode = "system" | "zh-CN" | "en";

const normalizeLocale = (value: string) => value.trim().toLowerCase();

const resolveSystemLocale = () => {
  const candidates = [
    navigator.languages?.[0],
    navigator.language,
  ].filter((value): value is string => Boolean(value && value.trim()));

  for (const candidate of candidates) {
    if (normalizeLocale(candidate).startsWith("zh")) {
      return "zh-CN";
    }
  }

  return "en";
};

const resolveLocale = (mode: LocaleMode): "zh-CN" | "en" => {
  if (mode === "system") {
    return resolveSystemLocale();
  }

  return mode;
};

const loadLocaleMode = (): LocaleMode => {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "system" || stored === "zh-CN" || stored === "en") {
    return stored;
  }

  return "system";
};

export const localeMode = ref<LocaleMode>(loadLocaleMode());
const locale = resolveLocale(localeMode.value);
document.documentElement.lang = locale;

const i18n = createI18n({
  legacy: false,
  locale,
  fallbackLocale: "en",
  messages: {
    "zh-CN": zhCN,
    en,
  },
});

export const setLocale = (mode: LocaleMode) => {
  localeMode.value = mode;
  const resolved = resolveLocale(mode);
  i18n.global.locale.value = resolved;
  localStorage.setItem(STORAGE_KEY, mode);
  document.documentElement.lang = resolved;
};

export default i18n;
