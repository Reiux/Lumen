import { nextTick, onMounted, onUnmounted, toValue, watch, type MaybeRefOrGetter, type Ref } from 'vue'

export interface DialogFocusTrapOptions {
  onEscape?: () => void
  initialFocus?: () => { focus?: (opts?: FocusOptions) => void } | null | undefined
}

const FOCUSABLE_SELECTOR = [
  'button:not([disabled])',
  '[href]',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"]):not([disabled])',
].join(', ')

function getFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter((el) => {
    if (el.hasAttribute('hidden')) return false
    const style = getComputedStyle(el)
    return style.display !== 'none' && style.visibility !== 'hidden'
  })
}

function isTopmostDialog(container: HTMLElement): boolean {
  const dialogs = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]'))
  return dialogs.length === 0 || dialogs[dialogs.length - 1] === container
}

export function useDialogFocusTrap(
  isOpen: MaybeRefOrGetter<boolean>,
  containerRef: Ref<HTMLElement | null>,
  options: DialogFocusTrapOptions = {},
) {
  let openerElement: HTMLElement | null = null

  function restoreOpener() {
    if (openerElement && typeof openerElement.focus === 'function') {
      openerElement.focus({ preventScroll: true })
    }
    openerElement = null
  }

  function focusDialog() {
    const container = containerRef.value
    if (!container) return
    const target = options.initialFocus?.()
    if (target && typeof target.focus === 'function') {
      target.focus({ preventScroll: true })
      if (document.activeElement && container.contains(document.activeElement)) {
        return
      }
    }
    const focusables = getFocusableElements(container)
    if (focusables.length > 0) {
      focusables[0].focus({ preventScroll: true })
    } else {
      container.focus({ preventScroll: true })
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!toValue(isOpen)) return
    const container = containerRef.value
    if (!container || !isTopmostDialog(container)) return

    if (e.key === 'Escape') {
      if (options.onEscape) {
        e.preventDefault()
        e.stopPropagation()
        options.onEscape()
      }
      return
    }

    if (e.key === 'Tab') {
      const focusables = getFocusableElements(container)
      if (focusables.length === 0) {
        e.preventDefault()
        container.focus({ preventScroll: true })
        return
      }
      const first = focusables[0]
      const last = focusables[focusables.length - 1]
      const active = document.activeElement as HTMLElement | null

      if (e.shiftKey) {
        if (!active || !container.contains(active) || active === first) {
          e.preventDefault()
          last.focus({ preventScroll: true })
        }
      } else if (!active || !container.contains(active) || active === last) {
        e.preventDefault()
        first.focus({ preventScroll: true })
      }
    }
  }

  watch(
    () => Boolean(toValue(isOpen)),
    (open, wasOpen) => {
      if (open && !wasOpen) {
        openerElement = (document.activeElement as HTMLElement) || null
        nextTick(() => focusDialog())
      } else if (!open && wasOpen) {
        restoreOpener()
      }
    },
  )

  onMounted(() => {
    window.addEventListener('keydown', handleKeydown, true)
    if (toValue(isOpen)) {
      openerElement = (document.activeElement as HTMLElement) || null
      nextTick(() => focusDialog())
    }
  })

  onUnmounted(() => {
    window.removeEventListener('keydown', handleKeydown, true)
    if (toValue(isOpen)) {
      restoreOpener()
    }
  })
}
