import { createI18n } from 'vue-i18n'
import { ConfigManager } from '../../bindings/app/core'
import en from './locales/en.json'
import es from './locales/es.json'
import id from './locales/id.json'
import ja from './locales/ja.json'

const savedLocale = typeof localStorage !== 'undefined' ? localStorage.getItem('lumen_locale') || 'en' : 'en'

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
  try {
    ConfigManager.SetLocale(locale).catch(() => {})
  } catch {
    // ignore if runtime not initialized
  }
}
