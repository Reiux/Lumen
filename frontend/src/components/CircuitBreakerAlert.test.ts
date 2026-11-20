import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { i18n } from '../i18n'
import { uploadManager } from '../utils/UploadManager'
import CircuitBreakerAlert from './CircuitBreakerAlert.vue'

vi.mock('../../bindings/app/core', () => ({
  ConfigManager: {
    RemoveCredentials: vi.fn().mockResolvedValue(undefined),
  },
}))

vi.mock('@wailsio/runtime', () => ({
  Browser: { OpenURL: vi.fn() },
  Clipboard: { SetText: vi.fn() },
  Events: { On: vi.fn().mockReturnValue(() => {}), Emit: vi.fn() },
}))

describe('CircuitBreakerAlert', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-11-19T12:00:00Z'))
    uploadManager.clearAccountBlocked()
  })

  afterEach(() => {
    uploadManager.clearAccountBlocked()
    vi.useRealTimers()
  })

  it('displays a live 30s countdown for RATE_LIMITED with no dismiss button and auto-clears at 0s', async () => {
    uploadManager.state.accountBlocked = {
      isBlocked: true,
      reason: 'RATE_LIMITED',
      email: 'user@example.com',
      message: 'HTTP 429',
      resumeAt: Date.now() + 30_000,
    }

    const wrapper = mount(CircuitBreakerAlert, {
      global: {
        plugins: [i18n],
        stubs: { Teleport: true },
      },
    })

    expect(wrapper.text()).toContain('30s')
    expect(wrapper.text()).not.toContain('Dismiss')
    expect(uploadManager.isUploadBlocked()).toBe(true)

    await vi.advanceTimersByTimeAsync(5_000)
    expect(wrapper.text()).toContain('25s')

    await vi.advanceTimersByTimeAsync(25_500)
    expect(uploadManager.state.accountBlocked.isBlocked).toBe(false)
    expect(uploadManager.isUploadBlocked()).toBe(false)

    wrapper.unmount()
  })

  it('keeps SIGN_IN_REQUIRED active until sign-in completes instead of clearing on click', async () => {
    uploadManager.state.accountBlocked = {
      isBlocked: true,
      reason: 'SIGN_IN_REQUIRED',
      email: 'user@example.com',
      message: 'HTTP 401',
    }

    const wrapper = mount(CircuitBreakerAlert, {
      global: {
        plugins: [i18n],
        stubs: { Teleport: true },
      },
    })

    const signInBtn = wrapper.findAll('button').find((b) => b.text().includes('Sign in again'))
    expect(signInBtn).toBeDefined()
    await signInBtn!.trigger('click')

    expect(wrapper.emitted('open-sign-in')).toBeTruthy()
    expect(uploadManager.state.accountBlocked.isBlocked).toBe(true)

    wrapper.unmount()
  })
})
