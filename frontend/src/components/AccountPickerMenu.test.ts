import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import AccountPickerMenu from './AccountPickerMenu.vue'

describe('AccountPickerMenu', () => {
  it('renders menu when open is true with role="menu"', async () => {
    const anchor = document.createElement('button')
    document.body.appendChild(anchor)

    const wrapper = mount(AccountPickerMenu, {
      props: {
        open: true,
        anchorEl: anchor,
        activeAccount: 'test@example.com',
        accounts: ['test@example.com', 'second@example.com'],
      },
      attachTo: document.body,
    })

    const menu = document.querySelector('[role="menu"]')
    expect(menu).not.toBeNull()
    expect(menu?.textContent).toContain('test@example.com')
    expect(menu?.textContent).toContain('second@example.com')
    expect(menu?.textContent).toContain('Manage accounts')
    expect(menu?.textContent).toContain('Cycle theme palette')

    wrapper.unmount()
    anchor.remove()
  })

  it('does not render menu when open is false', () => {
    const anchor = document.createElement('button')
    document.body.appendChild(anchor)

    const wrapper = mount(AccountPickerMenu, {
      props: {
        open: false,
        anchorEl: anchor,
        activeAccount: 'test@example.com',
        accounts: ['test@example.com'],
      },
      attachTo: document.body,
    })

    const menu = document.querySelector('[role="menu"]')
    expect(menu).toBeNull()

    wrapper.unmount()
    anchor.remove()
  })

  it('emits switch-account when an account is clicked', async () => {
    const anchor = document.createElement('button')
    document.body.appendChild(anchor)

    const wrapper = mount(AccountPickerMenu, {
      props: {
        open: true,
        anchorEl: anchor,
        activeAccount: 'first@example.com',
        accounts: ['first@example.com', 'second@example.com'],
      },
      attachTo: document.body,
    })

    const buttons = document.querySelectorAll<HTMLButtonElement>('[role="menu"] button')
    const secondAccountBtn = Array.from(buttons).find((b) => b.textContent?.includes('second@example.com'))
    expect(secondAccountBtn).toBeDefined()

    secondAccountBtn?.click()
    expect(wrapper.emitted('switch-account')?.[0]).toEqual(['second@example.com'])

    wrapper.unmount()
    anchor.remove()
  })

  it('emits close on Escape keydown', async () => {
    const anchor = document.createElement('button')
    document.body.appendChild(anchor)

    const wrapper = mount(AccountPickerMenu, {
      props: {
        open: true,
        anchorEl: anchor,
        activeAccount: 'test@example.com',
        accounts: ['test@example.com'],
      },
      attachTo: document.body,
    })

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(wrapper.emitted('close')).toBeTruthy()

    wrapper.unmount()
    anchor.remove()
  })
})
