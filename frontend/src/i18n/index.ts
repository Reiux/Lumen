import { createI18n } from 'vue-i18n'
import en from './locales/en.json'
import es from './locales/es.json'
import id from './locales/id.json'
import ja from './locales/ja.json'

const rawSavedLocale = typeof localStorage !== 'undefined' ? localStorage.getItem('lumen_locale') || 'en' : 'en'
const savedLocale = rawSavedLocale === 'es' ? 'en' : rawSavedLocale

export const i18n = createI18n({
  legacy: false,
  locale: savedLocale,
  fallbackLocale: 'en',
  messages: {
    en,
    es,
    id,
    ja,
  },
})

export function setLocale(locale: string) {
  (i18n.global.locale as any).value = locale
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem('lumen_locale', locale)
  }
}
