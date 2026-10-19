import type { Directive } from 'vue'

interface RippleElement extends HTMLElement {
  _rippleCleanup?: () => void
}

export const vRipple: Directive<RippleElement> = {
  mounted(el) {
    function createRipple(x: number, y: number) {
      if (el.hasAttribute('disabled') || el.getAttribute('aria-disabled') === 'true') {
        return
      }

      const rect = el.getBoundingClientRect()
      if (rect.width === 0 || rect.height === 0) return

      // Ensure positioning context & bounding
      if (getComputedStyle(el).position === 'static') {
        el.style.position = 'relative'
      }
      el.style.overflow = 'hidden'

      // Radius to furthest corner
      const maxDistX = Math.max(x, rect.width - x)
      const maxDistY = Math.max(y, rect.height - y)
      const radius = Math.sqrt(maxDistX * maxDistX + maxDistY * maxDistY)
      const diameter = radius * 2

      const ripple = document.createElement('span')
      ripple.className = 'm3-ripple'
      ripple.style.width = `${diameter}px`
      ripple.style.height = `${diameter}px`
      ripple.style.left = `${x - radius}px`
      ripple.style.top = `${y - radius}px`

      el.appendChild(ripple)

      const removeRipple = () => {
        ripple.removeEventListener('animationend', removeRipple)
        if (ripple.parentNode === el) {
          el.removeChild(ripple)
        }
      }

      ripple.addEventListener('animationend', removeRipple)
      // Fallback removal if animationend does not fire
      setTimeout(removeRipple, 500)
    }

    const onPointerDown = (e: PointerEvent) => {
      // Only primary mouse button (or touch)
      if (e.button !== 0) return
      const rect = el.getBoundingClientRect()
      createRipple(e.clientX - rect.left, e.clientY - rect.top)
    }

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.repeat) return
      if (e.key === 'Enter' || e.key === ' ') {
        const rect = el.getBoundingClientRect()
        createRipple(rect.width / 2, rect.height / 2)
      }
    }

    el.addEventListener('pointerdown', onPointerDown)
    el.addEventListener('keydown', onKeyDown)

    el._rippleCleanup = () => {
      el.removeEventListener('pointerdown', onPointerDown)
      el.removeEventListener('keydown', onKeyDown)
    }
  },

  unmounted(el) {
    if (el._rippleCleanup) {
      el._rippleCleanup()
      delete el._rippleCleanup
    }
  },
}
