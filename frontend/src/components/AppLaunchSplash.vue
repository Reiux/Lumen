<script lang="ts">
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
import LumenLogo from './LumenLogo.vue'
import { launchReplayTrigger } from '../utils/launchState'

const props = withDefaults(defineProps<{
  targetId?: string
  ready?: boolean
}>(), {
  targetId: 'rail-app-logo',
  ready: true,
})

const emit = defineEmits<{
  (e: 'settle', stagger?: number): void
  (e: 'finish'): void
  (e: 'start'): void
}>()

const isVisible = ref(true)
const isSwirling = ref(false)
const splashOpacity = ref(1)
const splashTransition = ref('none')
const logoOpacity = ref(1)
const logoTransform = ref('none')
const logoTransition = ref('none')
const splashLogoEl = ref<HTMLElement | null>(null)

let hasEnded = false
let timerIds: number[] = []
let startTime = 0
let isHandoffActive = false

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
  emit('settle', 40)
  emit('finish')
  isVisible.value = false
  isHandoffActive = false
}

function onSkip() {
  finish()
}

function cleanupEvents() {
  window.removeEventListener('keydown', onSkip)
  window.removeEventListener('pointerdown', onSkip)
  window.removeEventListener('resize', updateHandoffTransform)
}

function updateHandoffTransform() {
  if (!isHandoffActive || !splashLogoEl.value) return
  const targetEl = document.getElementById(props.targetId)
  if (!targetEl) return

  const toRect = targetEl.getBoundingClientRect()
  const fromX = window.innerWidth / 2
  const fromY = window.innerHeight / 2
  const toX = toRect.left + toRect.width / 2
  const toY = toRect.top + toRect.height / 2
  const scale = toRect.width / 160

  const dx = toX - fromX
  const dy = toY - fromY

  logoTransform.value = `translate(${dx}px, ${dy}px) scale(${scale})`
}

async function prepareWindow() {
  try {
    await Window.SetResizable(true)
    await Window.SetMinSize(800, 560)
    await Window.Center()
    await Window.SetTitle('Lumen')
  } catch {
    // Non-Wails / mock environment
  }

  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve())
    })
  })
}

async function startSequence(isReplay = false) {
  hasEnded = false
  isHandoffActive = false
  clearTimers()
  isVisible.value = true
  splashOpacity.value = 1
  splashTransition.value = 'none'
  logoOpacity.value = 1
  logoTransform.value = 'scale(1)'
  logoTransition.value = 'none'
  isSwirling.value = false
  startTime = Date.now()

  emit('start')

  if (!isReplay) {
    await prepareWindow()
  }

  // Accessibility: prefers-reduced-motion crossfades in <= 200ms
  if (typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) {
    splashTransition.value = 'opacity 180ms ease-out'
    splashOpacity.value = 0
    schedule(() => {
      finish()
    }, 180)
    return
  }

  window.addEventListener('keydown', onSkip, { once: true })
  window.addEventListener('pointerdown', onSkip, { once: true })
  window.addEventListener('resize', updateHandoffTransform)

  const mult = getLaunchMultiplier()
  const minDuration = 1200 * mult
  const maxWaitAfterReady = 2000 * mult
  const handoffDuration = 500 * mult

  // (b) Trigger blade swirl animation immediately
  requestAnimationFrame(() => {
    isSwirling.value = true
  })

  // (c) Wait until minDuration has elapsed AND app is ready
  function checkReadyAndProceed() {
    const elapsed = Date.now() - startTime
    const isAppReady = props.ready || elapsed >= (minDuration + maxWaitAfterReady)

    if (elapsed >= minDuration && isAppReady) {
      triggerHandoff(handoffDuration)
    } else {
      schedule(checkReadyAndProceed, 50)
    }
  }

  schedule(checkReadyAndProceed, minDuration)
}

function triggerHandoff(duration: number) {
  if (hasEnded || isHandoffActive) return
  isHandoffActive = true

  const targetEl = document.getElementById(props.targetId)
  if (targetEl && splashLogoEl.value) {
    const toRect = targetEl.getBoundingClientRect()
    const fromX = window.innerWidth / 2
    const fromY = window.innerHeight / 2
    const toX = toRect.left + toRect.width / 2
    const toY = toRect.top + toRect.height / 2
    const scale = toRect.width / 160

    const dx = toX - fromX
    const dy = toY - fromY

    logoTransition.value = `transform ${duration}ms ease-in-out`
    logoTransform.value = `translate(${dx}px, ${dy}px) scale(${scale})`
  }

  splashTransition.value = `opacity ${duration}ms ease-out`
  splashOpacity.value = 0

  schedule(() => {
    finish()
  }, duration)
}

// Watch for replay trigger from DevMockBar
watch(launchReplayTrigger, () => {
  startSequence(true)
})

onMounted(() => {
  // Skip launch if ?nolaunch=1 switch is set
  const urlParams = typeof window !== 'undefined' ? new URLSearchParams(window.location.search) : null
  const hasNoLaunchQuery = urlParams?.get('nolaunch') === '1'
  const hasNoLaunchAttr = typeof document !== 'undefined' && document.documentElement.hasAttribute('data-nolaunch')

  if (hasNoLaunchQuery || hasNoLaunchAttr) {
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
      class="fixed inset-0 z-50 flex items-center justify-center bg-[#141416] bg-[var(--md-sys-color-background)] pointer-events-none select-none"
      :style="{
        opacity: splashOpacity,
        transition: splashTransition,
      }"
      aria-hidden="true"
    >
      <div
        ref="splashLogoEl"
        class="flex items-center justify-center pointer-events-none"
        :style="{
          opacity: logoOpacity,
          transform: logoTransform,
          transition: logoTransition,
          willChange: isVisible ? 'transform, opacity' : 'auto',
        }"
      >
        <LumenLogo
          :size="160"
          :animated="isSwirling"
        />
      </div>
    </div>
  </Teleport>
</template>
