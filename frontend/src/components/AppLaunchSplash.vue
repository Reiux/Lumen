<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { SunMedium } from '@lucide/vue'

const props = withDefaults(defineProps<{
  targetId?: string
}>(), {
  targetId: 'sidebar-app-logo',
})

const emit = defineEmits<{
  (e: 'settle'): void
  (e: 'finish'): void
}>()

const isVisible = ref(true)
const splashOpacity = ref(1)
const logoOpacity = ref(0)
const logoTransform = ref('scale(0.85)')
const logoTransition = ref('none')
const splashLogoEl = ref<HTMLElement | null>(null)

let hasEnded = false
let timerIds: number[] = []

function schedule(fn: () => void, delay: number) {
  const id = window.setTimeout(fn, delay)
  timerIds.push(id)
  return id
}

function finish() {
  if (hasEnded) return
  hasEnded = true
  timerIds.forEach(id => clearTimeout(id))
  timerIds = []
  cleanupEvents()
  emit('settle')
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

onMounted(() => {
  // Check if already launched in this page session (e.g. Vite HMR update)
  const win = window as unknown as { __LUMEN_LAUNCHED__?: boolean }
  if (win.__LUMEN_LAUNCHED__) {
    isVisible.value = false
    emit('finish')
    return
  }
  win.__LUMEN_LAUNCHED__ = true

  // Check prefers-reduced-motion
  if (window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) {
    emit('settle')
    emit('finish')
    isVisible.value = false
    return
  }

  window.addEventListener('keydown', onSkip, { once: true })
  window.addEventListener('pointerdown', onSkip, { once: true })

  // Sequence:
  // Step 1: Logo mark sits centered at ~0.85 scale and fades in (~120ms, effects spring).
  requestAnimationFrame(() => {
    logoTransition.value = 'opacity 120ms var(--md-sys-motion-easing-effects)'
    logoOpacity.value = 1

    // Step 2: Scales up to 1.0 with a gentle spatial spring, ~250ms.
    schedule(() => {
      logoTransition.value = 'transform 250ms var(--md-sys-motion-easing-spatial)'
      logoTransform.value = 'scale(1.0)'

      // Step 3: Shared-element handoff (FLIP), ~350ms.
      schedule(() => {
        const targetEl = document.getElementById(props.targetId)
        if (targetEl && splashLogoEl.value) {
          const fromRect = splashLogoEl.value.getBoundingClientRect()
          const toRect = targetEl.getBoundingClientRect()

          const dx = (toRect.left + toRect.width / 2) - (fromRect.left + fromRect.width / 2)
          const dy = (toRect.top + toRect.height / 2) - (fromRect.top + fromRect.height / 2)
          const scale = toRect.width / fromRect.width

          logoTransition.value = 'transform 350ms var(--md-sys-motion-easing-spatial)'
          logoTransform.value = `translate(${dx}px, ${dy}px) scale(${scale})`
        }

        // Step 4: As it lands (~200ms into the glide), splash background fades out (~150ms)
        // and UI starts settling in
        schedule(() => {
          splashOpacity.value = 0
          emit('settle')
        }, 200)

        // Complete handoff and unmount at ~350ms
        schedule(() => {
          finish()
        }, 350)
      }, 250)
    }, 120)
  })
})

onUnmounted(() => {
  cleanupEvents()
  timerIds.forEach(id => clearTimeout(id))
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="isVisible"
      class="fixed inset-0 z-40 flex items-center justify-center bg-[var(--md-sys-color-surface)] pointer-events-none select-none transition-opacity duration-150 ease-out"
      :style="{ opacity: splashOpacity }"
      aria-hidden="true"
    >
      <div
        ref="splashLogoEl"
        class="flex size-14 items-center justify-center rounded-[19px] bg-[var(--md-sys-color-primary)] text-[var(--md-sys-color-on-primary)]"
        :style="{
          opacity: logoOpacity,
          transform: logoTransform,
          transition: logoTransition,
          willChange: 'transform, opacity',
        }"
      >
        <SunMedium class="size-8" />
      </div>
    </div>
  </Teleport>
</template>
