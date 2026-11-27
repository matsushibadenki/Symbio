import en from './locales/en.js';
import ja from './locales/ja.js';
import zhCN from './locales/zh-CN.js';

const words = { en, ja, 'zh-CN': zhCN };

function initialLocale(languages) {
  for (const language of languages) {
    const tag = language.toLowerCase();
    if (tag === 'ja' || tag.startsWith('ja-')) return 'ja';
    if (tag === 'zh' || tag.startsWith('zh-')) return 'zh-CN';
    if (tag === 'en' || tag.startsWith('en-')) return 'en';
  }
  return 'en';
}

export let locale;
export function setLocale(value) {
  locale = Object.hasOwn(words, value) ? value : 'en';
  document.documentElement.lang = locale;
}

setLocale(initialLocale(navigator.languages?.length ? navigator.languages : [navigator.language || 'en']));

export function t(key, values = {}) {
  const text = words[locale][key] ?? words.en[key] ?? key;
  return text.replace(/\{(\w+)\}/g, (placeholder, name) =>
    Object.hasOwn(values, name) ? String(values[name]) : placeholder);
}
