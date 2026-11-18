import { defineComponent, nextTick, ref } from 'vue'
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { useDialogFocusTrap } from './useDialogFocusTrap'

const DummyDialog = defineComponent({
  setup() {
    const open = ref(false)
    const cardRef = ref<HTMLElement | null>(null)
    useDialogFocusTrap(() => open.value, cardRef, {
      onEscape: () => {
        open.value = false
      },
    })
    return { open, cardRef }
  },
  template: `
    <div>
      <button id="opener" type="button" @click="open = true">Open</button>
      <div v-if="open" ref="cardRef" role="dialog" aria-modal="true" tabindex="-1">
        <button id="first" type="button">First</button>
        <button id="last" type="button" @click="open = false">Last</button>
      </div>
    </div>
  `,
})

describe('useDialogFocusTrap', () => {
  it('moves focus in on open, wraps Tab, closes on Escape, and restores opener focus', async () => {
    const wrapper = mount(DummyDialog, { attachTo: document.body })
    const opener = document.getElementById('opener') as HTMLButtonElement
    opener.focus()
    expect(document.activeElement).toBe(opener)

    await wrapper.find('#opener').trigger('click')
    await nextTick()

    const first = document.getElementById('first') as HTMLButtonElement
    const last = document.getElementById('last') as HTMLButtonElement
    expect(document.activeElement).toBe(first)

    last.focus()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    expect(document.activeElement).toBe(first)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true }))
    expect(document.activeElement).toBe(last)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(opener)

    wrapper.unmount()
  })
})
