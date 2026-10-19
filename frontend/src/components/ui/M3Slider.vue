<script setup lang="ts">
import { computed, ref } from 'vue'

interface Props {
  modelValue: number
  min?: number
  max?: number
  step?: number
  ariaLabel?: string
}

const props = withDefaults(defineProps<Props>(), {
  min: 1,
  max: 20,
  step: 1,
  ariaLabel: 'Slider',
})

const emit = defineEmits<{
  (e: 'update:modelValue', val: number): void
}>()

const isDragging = ref(false)
const isHovered = ref(false)
const trackRef = ref<HTMLDivElement | null>(null)

const percent = computed(() => {
  const range = props.max - props.min
  if (range <= 0) return 0
  const clamped = Math.min(props.max, Math.max(props.min, props.modelValue))
  return ((clamped - props.min) / range) * 100
})

function updateFromPointer(clientX: number) {
  if (!trackRef.value) return
  const rect = trackRef.value.getBoundingClientRect()
  const rawX = clientX - rect.left
  const ratio = Math.min(1, Math.max(0, rawX / rect.width))
  const rawVal = props.min + ratio * (props.max - props.min)
  const steppedVal = Math.round(rawVal / props.step) * props.step
  const clamped = Math.min(props.max, Math.max(props.min, steppedVal))
  emit('update:modelValue', clamped)
}

function onPointerDown(e: PointerEvent) {
  // Only primary mouse button
  if (e.button !== 0) return
  isDragging.value = true
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  updateFromPointer(e.clientX)
}

function onPointerMove(e: PointerEvent) {
  if (!isDragging.value) return
  updateFromPointer(e.clientX)
}

function onPointerUp(e: PointerEvent) {
  if (isDragging.value) {
    isDragging.value = false
    try {
      ;(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId)
    } catch {
      // ignore
    }
  }
}

function onKeyDown(e: KeyboardEvent) {
  let delta = 0
  if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
    delta = props.step
  } else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
    delta = -props.step
  } else if (e.key === 'PageUp') {
    delta = props.step * 4
  } else if (e.key === 'PageDown') {
    delta = -props.step * 4
  } else if (e.key === 'Home') {
    emit('update:modelValue', props.min)
    return
  } else if (e.key === 'End') {
    emit('update:modelValue', props.max)
    return
  }
  if (delta !== 0) {
    e.preventDefault()
    const next = Math.min(props.max, Math.max(props.min, props.modelValue + delta))
    emit('update:modelValue', next)
  }
}
</script>

<template>
  <div
    class="relative flex w-full flex-col justify-center select-none pt-6 pb-2"
    style="--wails-draggable: none"
  >
    <!-- Value Label Bubble: follows thumb, pops up with spring -->
    <div
      class="pointer-events-none absolute -top-1 transition-all"
      :style="{
        left: `${percent}%`,
        transform: 'translateX(-50%)',
        transitionProperty: isDragging ? 'opacity, transform' : 'all',
        transitionDuration: 'var(--md-sys-motion-duration-spatial)',
        transitionTimingFunction: 'var(--md-sys-motion-easing-spatial)',
      }"
    >
      <div
        class="flex items-center justify-center rounded-full bg-[var(--md-sys-color-primary)] px-2.5 py-0.5 text-[11px] font-bold text-[var(--md-sys-color-on-primary)] shadow-md transition-all"
        :class="[
          isDragging || isHovered
            ? 'scale-100 opacity-100 translate-y-0'
            : 'scale-50 opacity-0 translate-y-2',
        ]"
        :style="{
          transitionTimingFunction: 'var(--md-sys-motion-easing-spatial)',
          transitionDuration: 'var(--md-sys-motion-duration-spatial-fast)',
        }"
      >
        {{ modelValue }}
      </div>
    </div>

    <!-- Interactive Track Area -->
    <div
      ref="trackRef"
      role="slider"
      tabindex="0"
      :aria-valuemin="min"
      :aria-valuemax="max"
      :aria-valuenow="modelValue"
      :aria-label="ariaLabel"
      class="group relative flex h-10 w-full cursor-pointer items-center outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)] rounded-full"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @mouseenter="isHovered = true"
      @mouseleave="isHovered = false"
      @keydown="onKeyDown"
    >
      <!-- Background / Inactive Track (Right side) -->
      <div
        class="absolute right-0 h-2 rounded-r-full bg-[var(--md-sys-color-surface-container-highest)] transition-all"
        :style="{
          left: `calc(${percent}% + ${isDragging ? '6px' : '4px'})`,
          transitionDuration: 'var(--md-sys-motion-duration-spatial-fast)',
          transitionTimingFunction: 'var(--md-sys-motion-easing-spatial)',
        }"
      />

      <!-- Active Track (Left side) -->
      <div
        class="absolute left-0 h-2 rounded-l-full bg-[var(--md-sys-color-primary)] transition-all"
        :style="{
          width: `calc(${percent}% - ${isDragging ? '6px' : '4px'})`,
          transitionDuration: 'var(--md-sys-motion-duration-spatial-fast)',
          transitionTimingFunction: 'var(--md-sys-motion-easing-spatial)',
        }"
      />

      <!-- Handle Thumb: narrows on press (thin pill handle like M3E) -->
      <div
        class="pointer-events-none absolute top-1/2 flex -translate-x-1/2 -translate-y-1/2 items-center justify-center transition-all"
        :style="{
          left: `${percent}%`,
          transitionProperty: isDragging ? 'width, height' : 'all',
          transitionDuration: 'var(--md-sys-motion-duration-spatial)',
          transitionTimingFunction: 'var(--md-sys-motion-easing-spatial)',
        }"
      >
        <div
          class="rounded-full shadow-md transition-all"
          :class="[
            isDragging
              ? 'w-1 h-8 bg-[var(--md-sys-color-on-primary)] ring-4 ring-[var(--md-sys-color-primary)]/40'
              : 'w-2.5 h-6 bg-[var(--md-sys-color-primary)] group-hover:scale-110',
          ]"
          :style="{
            transitionDuration: 'var(--md-sys-motion-duration-spatial-fast)',
            transitionTimingFunction: 'var(--md-sys-motion-easing-spatial)',
          }"
        />
      </div>
    </div>
  </div>
</template>
