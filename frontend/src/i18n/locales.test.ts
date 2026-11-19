import { describe, expect, it } from 'vitest'
import { createI18n } from 'vue-i18n'
import en from './locales/en.json'
import es from './locales/es.json'
import id from './locales/id.json'
import ja from './locales/ja.json'

const locales = { en, es, id, ja } as const

function collectMessageKeys(obj: Record<string, unknown>, prefix = ''): Array<[string, string]> {
  const entries: Array<[string, string]> = []
  for (const [key, value] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key
    if (typeof value === 'string') {
      entries.push([fullKey, value])
    } else if (value && typeof value === 'object') {
      entries.push(...collectMessageKeys(value as Record<string, unknown>, fullKey))
    }
  }
  return entries
}

describe('i18n locale bundles', () => {
  it('compiles every message in all locales without vue-i18n syntax errors', () => {
    const i18n = createI18n({
      legacy: false,
      locale: 'en',
      messages: locales,
    })

    for (const [locale, messages] of Object.entries(locales)) {
      i18n.global.locale.value = locale as keyof typeof locales
      for (const [key] of collectMessageKeys(messages as Record<string, unknown>)) {
        expect(() => i18n.global.t(key), `[${locale}] ${key}`).not.toThrow()
      }
    }
  })

  it('renders settings.excludePatternSubtitle with literal regex pipe and @eaDir in all locales', () => {
    const i18n = createI18n({
      legacy: false,
      locale: 'en',
      messages: locales,
    })

    for (const locale of Object.keys(locales) as Array<keyof typeof locales>) {
      i18n.global.locale.value = locale
      const rendered = i18n.global.t('settings.excludePatternSubtitle')
      expect(rendered).toContain('\\.tmp$|@eaDir')
    }
  })
})
