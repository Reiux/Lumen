/**
 * Global input modality tracker.
 * Distinguishes between keyboard and pointer (mouse/touch) interaction,
 * preventing unwanted focus rings after mouse clicks while preserving
 * accessibility for keyboard navigation.
 */
export function initInputModality(): void {
  if (typeof document === 'undefined') return

  // Default to pointer
  document.documentElement.setAttribute('data-input', 'pointer')

  const keyboardKeys = new Set([
    'Tab',
    'Enter',
    'Escape',
    ' ',
    'Space',
    'Spacebar',
    'ArrowUp',
    'ArrowDown',
    'ArrowLeft',
    'ArrowRight',
    'Home',
    'End',
    'PageUp',
    'PageDown',
  ])

  window.addEventListener(
    'keydown',
    (e: KeyboardEvent) => {
      if (keyboardKeys.has(e.key) || e.key.startsWith('Arrow')) {
        document.documentElement.setAttribute('data-input', 'keyboard')
      }
    },
    { capture: true },
  )

  const setPointer = () => {
    document.documentElement.setAttribute('data-input', 'pointer')
  }

  window.addEventListener('pointerdown', setPointer, { capture: true })
  window.addEventListener('mousedown', setPointer, { capture: true })
}
