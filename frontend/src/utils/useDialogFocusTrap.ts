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

interface ActiveTrapEntry {
  containerRef: Ref<HTMLElement | null>
}

const DIALOG_ROLE_SELECTOR = '[role="dialog"], [role="alertdialog"]'
const activeTrapStack: ActiveTrapEntry[] = []
const managedContainers = new WeakSet<HTMLElement>()

function pushTrap(entry: ActiveTrapEntry) {
  const idx = activeTrapStack.indexOf(entry)
  if (idx !== -1) {
    activeTrapStack.splice(idx, 1)
  }
  activeTrapStack.push(entry)
}

function removeTrap(entry: ActiveTrapEntry) {
  const idx = activeTrapStack.indexOf(entry)
  if (idx !== -1) {
    activeTrapStack.splice(idx, 1)
  }
}

function isTopmostDialog(entry: ActiveTrapEntry, container: HTMLElement): boolean {
  if (activeTrapStack.length > 0) {
    if (activeTrapStack[activeTrapStack.length - 1] !== entry) {
      return false
    }
    const domDialogs = Array.from(document.querySelectorAll<HTMLElement>(DIALOG_ROLE_SELECTOR)).filter(
      (el) => !managedContainers.has(el) || activeTrapStack.some((t) => t.containerRef.value === el),
    )
    if (domDialogs.length > 0) {
      const lastDom = domDialogs[domDialogs.length - 1]
      if (lastDom !== container && !managedContainers.has(lastDom)) {
        return false
      }
    }
    return true
  }
  const dialogs = Array.from(document.querySelectorAll<HTMLElement>(DIALOG_ROLE_SELECTOR))
  return dialogs.length === 0 || dialogs[dialogs.length - 1] === container
}

export function useDialogFocusTrap(
  isOpen: MaybeRefOrGetter<boolean>,
  containerRef: Ref<HTMLElement | null>,
  options: DialogFocusTrapOptions = {},
) {
  let openerElement: HTMLElement | null = null
  const trapEntry: ActiveTrapEntry = { containerRef }

  function restoreOpener() {
    if (openerElement && typeof openerElement.focus === 'function') {
      openerElement.focus({ preventScroll: true })
    }
    openerElement = null
  }

  function focusDialog() {
    const container = containerRef.value
    if (!container) return
    managedContainers.add(container)
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
    if (!container) return
    managedContainers.add(container)
    if (!isTopmostDialog(trapEntry, container)) return

    if (e.key === 'Escape') {
      if (options.onEscape) {
        e.preventDefault()
        e.stopPropagation()
        e.stopImmediatePropagation()
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
        pushTrap(trapEntry)
        nextTick(() => focusDialog())
      } else if (!open && wasOpen) {
        removeTrap(trapEntry)
        restoreOpener()
      }
    },
  )

  onMounted(() => {
    window.addEventListener('keydown', handleKeydown, true)
    if (toValue(isOpen)) {
      openerElement = (document.activeElement as HTMLElement) || null
      pushTrap(trapEntry)
      if (containerRef.value) {
        managedContainers.add(containerRef.value)
      }
      nextTick(() => focusDialog())
    }
  })

  onUnmounted(() => {
    window.removeEventListener('keydown', handleKeydown, true)
    removeTrap(trapEntry)
    if (toValue(isOpen)) {
      restoreOpener()
    }
  })
}
