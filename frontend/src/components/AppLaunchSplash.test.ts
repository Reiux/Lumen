import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

const emitMock = vi.fn().mockResolvedValue(undefined)
const setResizableMock = vi.fn().mockResolvedValue(undefined)
const setMinSizeMock = vi.fn().mockResolvedValue(undefined)
const centerMock = vi.fn().mockResolvedValue(undefined)
const setTitleMock = vi.fn().mockResolvedValue(undefined)

vi.mock('@wailsio/runtime', () => ({
  Events: {
    Emit: (...args: unknown[]) => emitMock(...args),
  },
  Window: {
    SetResizable: (...args: unknown[]) => setResizableMock(...args),
    SetMinSize: (...args: unknown[]) => setMinSizeMock(...args),
    Center: (...args: unknown[]) => centerMock(...args),
    SetTitle: (...args: unknown[]) => setTitleMock(...args),
  },
}))

import AppLaunchSplash from './AppLaunchSplash.vue'
import LumenLogo from './LumenLogo.vue'

describe('AppLaunchSplash', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    emitMock.mockClear()
    setResizableMock.mockClear()
    setMinSizeMock.mockClear()
    centerMock.mockClear()
    setTitleMock.mockClear()
    delete (window as unknown as { __LUMEN_LAUNCHED__?: boolean }).__LUMEN_LAUNCHED__

    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      return window.setTimeout(() => cb(performance.now()), 16)
    })
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it('reveals window via app:ready before starting the logo 360 spin animation', async () => {
    const wrapper = mount(AppLaunchSplash, {
      props: { ready: false },
      global: {
        stubs: {
          Teleport: true,
        },
      },
    })

    // Immediately after mount, neither app:ready nor the spin should have fired yet
    expect(wrapper.findComponent(LumenLogo).props('animated')).toBe(false)

    // Advance past Window IPC + initial double-rAF so app:ready is emitted
    await vi.advanceTimersByTimeAsync(40)
    await flushPromises()
    expect(emitMock).toHaveBeenCalledWith('app:ready')

    // During the post-show macOS compositor settle window, spin has not started yet
    expect(wrapper.findComponent(LumenLogo).props('animated')).toBe(false)

    // Advance past the 80ms settle + frames -> spin begins while window is visible
    await vi.advanceTimersByTimeAsync(150)
    await flushPromises()
    expect(wrapper.findComponent(LumenLogo).props('animated')).toBe(true)

    wrapper.unmount()
  })
})
