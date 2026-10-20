<script lang="ts">
export const TIMING = {
  fadeIn: 280,  // 0-280ms: smooth fade in with scale 0.85 -> 0.94 (effects spring)
  settle: 360,  // 280-640ms: graceful spatial spring settling into 1.0
  hold: 240,    // 640-880ms: deliberate hold so the logo is clearly visible
  handoff: 420, // 880-1300ms: smooth cinematic FLIP glide and corner morph to #sidebar-app-logo
  stagger: 45,  // UI settle stagger between card, nav items, and main pane
}

export function getLaunchMultiplier(): number {
  if (typeof window === 'undefined') return 1
  const urlParams = new URLSearchParams(window.location.search)
  const slowParam = urlParams.get('launchSlow')
  if (slowParam) {
    const val = parseFloat(slowParam)
    if (!Number.isNaN(val) && val > 0) return val
  }
  const win = window as unknown as { __LUMEN_LAUNCH_SLOW__?: number }
  if (typeof win.__LUMEN_LAUNCH_SLOW__ === 'number' && win.__LUMEN_LAUNCH_SLOW__ > 0) {
    return win.__LUMEN_LAUNCH_SLOW__
  }
  return 1
}
</script>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { Window } from '@wailsio/runtime'
import M3Icon from './ui/M3Icon.vue'
import { launchReplayTrigger } from '../utils/launchState'

const props = withDefaults(defineProps<{
  targetId?: string
}>(), {
  targetId: 'sidebar-app-logo',
})

const emit = defineEmits<{
  (e: 'settle', stagger?: number): void
  (e: 'finish'): void
  (e: 'start'): void
}>()

const isVisible = ref(true)
const splashOpacity = ref(1)
const splashTransition = ref('none')
const logoOpacity = ref(0)
const logoTransform = ref('scale(0.88)')
const logoBorderRadius = ref('32px')
const logoTransition = ref('none')
const splashLogoEl = ref<HTMLElement | null>(null)

let hasEnded = false
let timerIds: number[] = []

function schedule(fn: () => void, delay: number) {
  const id = window.setTimeout(fn, delay)
  timerIds.push(id)
  return id
}

function clearTimers() {
  timerIds.forEach(id => clearTimeout(id))
  timerIds = []
}

function finish() {
  if (hasEnded) return
  hasEnded = true
  clearTimers()
  cleanupEvents()
  if (splashLogoEl.value) {
    splashLogoEl.value.style.willChange = 'auto'
  }
  const mult = getLaunchMultiplier()
  emit('settle', TIMING.stagger * mult)
  emit('finish')
  isVisible.value = false
}

function onSkip() {
  finish()
}

function cleanupEvents() {
  window.removeEventListener('keydown', onSkip)
  window.removeEventListener('pointerdown', onSkip)
}

async function prepareWindow() {
  try {
    await Window.SetResizable(true)
    await Window.SetMinSize(800, 560)
    await Window.Center()
    await Window.SetTitle('Lumen')
  } catch {
    // Non-Wails/mock environment or already set
  }

  // Wait two requestAnimationFrame ticks so layout/viewport dimensions stabilize
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve())
    })
  })
}

async function startSequence(isReplay = false) {
  hasEnded = false
  clearTimers()
  isVisible.value = true
  splashOpacity.value = 1
  splashTransition.value = 'none'
  logoOpacity.value = 0
  logoTransform.value = 'scale(0.88)'
  logoBorderRadius.value = '32px'
  logoTransition.value = 'none'

  emit('start')

  if (!isReplay) {
    await prepareWindow()
  } else {
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => resolve())
      })
    })
  }

  // Check prefers-reduced-motion
  if (window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) {
    emit('settle', 0)
    emit('finish')
    isVisible.value = false
    return
  }

  window.addEventListener('keydown', onSkip, { once: true })
  window.addEventListener('pointerdown', onSkip, { once: true })

  const mult = getLaunchMultiplier()
  const dFadeIn = TIMING.fadeIn * mult
  const dSettle = TIMING.settle * mult
  const dHold = TIMING.hold * mult
  const dHandoff = TIMING.handoff * mult
  const dStagger = TIMING.stagger * mult
  const dSplashFade = 180 * mult

  // Timeline (smooth, deliberate ~1s cold-start):
  // Step 1: fade in + scale 0.85 -> 0.94 (effects spring)
  requestAnimationFrame(() => {
    logoTransition.value = `opacity ${dFadeIn}ms var(--md-sys-motion-easing-effects), transform ${dFadeIn}ms var(--md-sys-motion-easing-effects)`
    logoOpacity.value = 1
    logoTransform.value = 'scale(0.94)'

    // Step 2: scale to 1.0 with spatial spring
    schedule(() => {
      logoTransition.value = `transform ${dSettle}ms var(--md-sys-motion-easing-spatial)`
      logoTransform.value = 'scale(1.0)'

      // Step 3: hold still briefly
      schedule(() => {
        // Step 4: FLIP container transform glide to #sidebar-app-logo + corner morph
        const targetEl = document.getElementById(props.targetId)
        if (targetEl && splashLogoEl.value) {
          const fromRect = splashLogoEl.value.getBoundingClientRect()
          const toRect = targetEl.getBoundingClientRect()

          const dx = (toRect.left + toRect.width / 2) - (fromRect.left + fromRect.width / 2)
          const dy = (toRect.top + toRect.height / 2) - (fromRect.top + fromRect.height / 2)
          const scale = toRect.width / fromRect.width

          logoTransition.value = `transform ${dHandoff}ms var(--md-sys-motion-easing-spatial), border-radius ${dHandoff}ms var(--md-sys-motion-easing-spatial)`
          logoTransform.value = `translate(${dx}px, ${dy}px) scale(${scale})`
          logoBorderRadius.value = '14px'
        }

        // Splash background fades out during the handoff
        schedule(() => {
          splashTransition.value = `opacity ${dSplashFade}ms ease-out`
          splashOpacity.value = 0
          emit('settle', dStagger)
        }, Math.max(0, dHandoff - dSplashFade))

        // Complete handoff and unmount at dHandoff
        schedule(() => {
          finish()
        }, dHandoff)
      }, dHold)
    }, dFadeIn)
  })
}

// Watch for replay trigger from DevMockBar
watch(launchReplayTrigger, () => {
  startSequence(true)
})

onMounted(() => {
  // Skip launch if ?nolaunch=1 switch is set
  if (typeof document !== 'undefined' && document.documentElement.hasAttribute('data-nolaunch')) {
    isVisible.value = false
    emit('finish')
    return
  }

  // Check if already launched in this page session (e.g. Vite HMR update)
  const win = window as unknown as { __LUMEN_LAUNCHED__?: boolean }
  if (win.__LUMEN_LAUNCHED__) {
    isVisible.value = false
    emit('finish')
    return
  }
  win.__LUMEN_LAUNCHED__ = true

  startSequence(false)
})

onUnmounted(() => {
  cleanupEvents()
  clearTimers()
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="isVisible"
      class="fixed inset-0 z-40 flex items-center justify-center bg-[var(--md-sys-color-surface)] pointer-events-none select-none"
      :style="{
        opacity: splashOpacity,
        transition: splashTransition,
      }"
      aria-hidden="true"
    >
      <div
        ref="splashLogoEl"
        class="flex size-24 items-center justify-center bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)] shadow-md"
        :style="{
          opacity: logoOpacity,
          transform: logoTransform,
          borderRadius: logoBorderRadius,
          transition: logoTransition,
          willChange: isVisible ? 'transform, opacity, border-radius' : 'auto',
        }"
      >
        <M3Icon
          name="light_mode"
          :size="56"
        />
      </div>
    </div>
  </Teleport>
</template>
