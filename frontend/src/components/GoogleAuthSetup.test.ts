import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import GoogleAuthSetup from './GoogleAuthSetup.vue'
import { i18n } from '../i18n'

// Mock ConfigManager
vi.mock('../../bindings/app/core', () => ({
  ConfigManager: {
    IsInlineEmbeddedSignInSupported: vi.fn().mockResolvedValue(false),
    StartEmbeddedSignIn: vi.fn().mockResolvedValue('test@example.com'),
    UpdateEmbeddedSignInBounds: vi.fn().mockResolvedValue(undefined),
    StartExternalBrowserSignIn: vi.fn().mockResolvedValue('test@example.com'),
    CancelBrowserSignIn: vi.fn().mockResolvedValue(undefined),
    HasSupportedBrowser: vi.fn().mockResolvedValue(true),
    GetSupportedBrowserName: vi.fn().mockResolvedValue('Chrome'),
    SetAccountRiskAcknowledged: vi.fn().mockResolvedValue(undefined),
  },
}))

vi.mock('@wailsio/runtime', () => ({
  Browser: { OpenURL: vi.fn() },
  Clipboard: { Text: vi.fn().mockResolvedValue(''), SetText: vi.fn() },
  Events: { On: vi.fn().mockReturnValue(() => {}) },
}))

describe('GoogleAuthSetup', () => {
  it('does not close when clicking Direct Token / String button', async () => {
    let isOpen = true
    const wrapper = mount(GoogleAuthSetup, {
      props: {
        open: isOpen,
        'onUpdate:open': (val: boolean) => {
          isOpen = val
        },
      },
      global: {
        plugins: [i18n],
      },
      attachTo: document.body,
    })

    // Find the Direct Token button
    const buttons = Array.from(document.querySelectorAll('button'))
    const understandBtn = buttons.find((b) => b.textContent?.includes('understand') || b.textContent?.includes('entiendo'))
    if (understandBtn) {
      await understandBtn.click()
    }

    const manualToggleBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => b.textContent?.includes('manually') || b.textContent?.includes('manualmente')
    )
    if (manualToggleBtn) {
      await manualToggleBtn.click()
    }

    const directBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => b.textContent?.includes('Direct') || b.textContent?.includes('directo')
    )
    expect(directBtn).toBeDefined()
    await directBtn?.click()

    expect(wrapper.emitted('update:open')).toBeUndefined()
    expect(isOpen).toBe(true)

    wrapper.unmount()
  })
})
