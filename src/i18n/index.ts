import { createI18n } from "vue-i18n";
import zhCN from "../locales/zh-CN.json";
import en from "../locales/en.json";

const STORAGE_KEY = "seekmind-locale";

const loadLocale = (): string => {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "zh-CN" || stored === "en") {
    return stored;
  }
  const lang = navigator.language;
  if (lang.startsWith("zh")) {
    return "zh-CN";
  }
  return "en";
};

const locale = loadLocale();
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

export const setLocale = (lang: "zh-CN" | "en") => {
  i18n.global.locale.value = lang;
  localStorage.setItem(STORAGE_KEY, lang);
  document.documentElement.lang = lang;
};

export default i18n;
