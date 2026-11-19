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

  it('closes only the topmost stacked alertdialog on Escape, then closes the underlying dialog', async () => {
    const StackedDialogs = defineComponent({
      setup() {
        const baseOpen = ref(false)
        const alertOpen = ref(false)
        const baseRef = ref<HTMLElement | null>(null)
        const alertRef = ref<HTMLElement | null>(null)

        useDialogFocusTrap(() => baseOpen.value, baseRef, {
          onEscape: () => {
            baseOpen.value = false
          },
        })
        useDialogFocusTrap(() => alertOpen.value, alertRef, {
          onEscape: () => {
            alertOpen.value = false
          },
        })

        return { baseOpen, alertOpen, baseRef, alertRef }
      },
      template: `
        <div>
          <button id="open-base" type="button" @click="baseOpen = true">Open Base</button>
          <div v-if="baseOpen" ref="baseRef" role="dialog" aria-modal="true" tabindex="-1">
            <button id="open-alert" type="button" @click="alertOpen = true">Open Alert</button>
          </div>
          <div v-if="alertOpen" ref="alertRef" role="alertdialog" aria-modal="true" tabindex="-1">
            <button id="confirm-alert" type="button">Confirm</button>
          </div>
        </div>
      `,
    })

    const wrapper = mount(StackedDialogs, { attachTo: document.body })
    const openBase = document.getElementById('open-base') as HTMLButtonElement
    openBase.focus()

    await wrapper.find('#open-base').trigger('click')
    await nextTick()
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()

    await wrapper.find('#open-alert').trigger('click')
    await nextTick()
    expect(document.querySelector('[role="alertdialog"]')).not.toBeNull()
    expect(document.activeElement).toBe(document.getElementById('confirm-alert'))

    // First Escape closes only the topmost alertdialog; underlying dialog stays open
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()
    expect(document.querySelector('[role="alertdialog"]')).toBeNull()
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()

    // Second Escape closes the underlying dialog and restores focus to opener
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(openBase)

    wrapper.unmount()
  })
})
