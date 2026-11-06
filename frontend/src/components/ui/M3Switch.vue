<script setup lang="ts">
import { computed, ref } from 'vue'

interface Props {
  modelValue: boolean
  disabled?: boolean
  variant?: 'default' | 'danger'
  id?: string
  ariaLabel?: string
  showIcon?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  disabled: false,
  variant: 'default',
  id: undefined,
  ariaLabel: undefined,
  showIcon: true,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'change', value: boolean): void
}>()

const isPressed = ref(false)
let pressTimeout: ReturnType<typeof setTimeout> | null = null

function toggle() {
  if (props.disabled) return
  const next = !props.modelValue
  emit('update:modelValue', next)
  emit('change', next)
}

function onKeydown(e: KeyboardEvent) {
  if (props.disabled) return
  if (e.key === ' ' || e.key === 'Enter') {
    e.preventDefault()
    toggle()
  }
}

function onPointerDown() {
  if (props.disabled) return
  if (pressTimeout) {
    clearTimeout(pressTimeout)
    pressTimeout = null
  }
  isPressed.value = true
}

function onPointerUp() {
  if (pressTimeout) clearTimeout(pressTimeout)
  pressTimeout = setTimeout(() => {
    isPressed.value = false
    pressTimeout = null
  }, 150)
}

function onPointerLeave() {
  if (pressTimeout) {
    clearTimeout(pressTimeout)
    pressTimeout = null
  }
  isPressed.value = false
}

const isChecked = computed(() => props.modelValue)
</script>

<template>
  <button
    :id="id"
    type="button"
    role="switch"
    :aria-checked="isChecked"
    :aria-label="ariaLabel"
    :disabled="disabled"
    class="m3e-switch select-none cursor-pointer"
    :class="[
      isChecked ? 'is-checked' : 'is-unchecked',
      variant === 'danger' && 'is-danger',
      isPressed && 'is-pressed',
      disabled && 'is-disabled',
    ]"
    @click="toggle"
    @keydown="onKeydown"
    @pointerdown="onPointerDown"
    @pointerup="onPointerUp"
    @pointerleave="onPointerLeave"
    @pointercancel="onPointerLeave"
  >
    <!-- Track (52x32) -->
    <span class="m3e-switch__track">
      <!-- Handle Container -->
      <span class="m3e-switch__handle-container">
        <!-- State layer -->
        <span class="m3e-switch__state-layer" aria-hidden="true" />

        <!-- Thumb handle (24px at rest, 28px when pressed) -->
        <span class="m3e-switch__thumb">
          <!-- Icon inside thumb (Check when ON, Close when OFF) -->
          <svg
            v-if="showIcon"
            class="m3e-switch__icon"
            :class="isChecked ? 'is-checked-icon' : 'is-unchecked-icon'"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.75"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <polyline v-if="isChecked" points="20 6 9 17 4 12" />
            <g v-else>
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </g>
          </svg>
        </span>
      </span>
    </span>
  </button>
</template>

<style scoped>
.m3e-switch {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 32px;
  padding: 0;
  border: none;
  background: transparent;
  outline: none;
  border-radius: 9999px;
  vertical-align: middle;
  flex-shrink: 0;
  -webkit-tap-highlight-color: transparent;
}

.m3e-switch:focus-visible {
  outline: 3px solid var(--md-sys-color-primary);
  outline-offset: 2px;
}

/* Track: 52x32 */
.m3e-switch__track {
  position: relative;
  display: flex;
  align-items: center;
  width: 52px;
  height: 32px;
  border-radius: 9999px;
  box-sizing: border-box;
  transition:
    background-color 200ms cubic-bezier(0.4, 0, 0.2, 1),
    border-color 200ms cubic-bezier(0.4, 0, 0.2, 1);
}

/* OFF state: outlined track */
.m3e-switch.is-unchecked .m3e-switch__track {
  border: 2px solid var(--md-sys-color-outline);
  background-color: var(--md-sys-color-surface-container-highest);
}

/* ON state: filled track */
.m3e-switch.is-checked .m3e-switch__track {
  border: 2px solid transparent;
  background-color: var(--md-sys-color-primary);
}

.m3e-switch.is-checked.is-danger .m3e-switch__track {
  background-color: var(--md-sys-color-error);
}

/* Handle container handles position with M3 Expressive spring */
.m3e-switch__handle-container {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    left 250ms cubic-bezier(0.38, 1.21, 0.22, 1),
    transform 250ms cubic-bezier(0.38, 1.21, 0.22, 1);
}

/* OFF: thumb size = 24px (with icon), positioned at 4px from edge */
.m3e-switch.is-unchecked .m3e-switch__handle-container {
  left: 4px;
}

/* ON: thumb size = 24px, positioned so right edge has 4px margin: 52 - 24 - 4 = 24px */
.m3e-switch.is-checked .m3e-switch__handle-container {
  left: 24px;
}

/* Pressed state adjustments: 28px thumb */
.m3e-switch.is-unchecked.is-pressed .m3e-switch__handle-container {
  left: 2px;
}

.m3e-switch.is-checked.is-pressed .m3e-switch__handle-container {
  left: 20px;
}

/* State layer circle */
.m3e-switch__state-layer {
  position: absolute;
  width: 40px;
  height: 40px;
  border-radius: 9999px;
  background-color: currentColor;
  opacity: 0;
  pointer-events: none;
  transition: opacity 150ms ease;
}

.m3e-switch:hover:not(:disabled) .m3e-switch__state-layer {
  opacity: 0.08;
}

.m3e-switch:focus-visible .m3e-switch__state-layer {
  opacity: 0.10;
}

.m3e-switch:active:not(:disabled) .m3e-switch__state-layer,
.m3e-switch.is-pressed:not(:disabled) .m3e-switch__state-layer {
  opacity: 0.10;
}

/* Thumb Handle: 24px default, 28px on press */
.m3e-switch__thumb {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 9999px;
  box-sizing: border-box;
  transition:
    width 200ms cubic-bezier(0.38, 1.21, 0.22, 1),
    height 200ms cubic-bezier(0.38, 1.21, 0.22, 1),
    background-color 200ms ease,
    color 200ms ease;
}

/* OFF: 24px thumb, outline color with contrasting icon */
.m3e-switch.is-unchecked .m3e-switch__thumb {
  background-color: var(--md-sys-color-outline);
  color: var(--md-sys-color-surface-container-highest);
}

/* ON: 24px thumb WITH check icon */
.m3e-switch.is-checked .m3e-switch__thumb {
  background-color: var(--md-sys-color-on-primary);
  color: var(--md-sys-color-primary);
}

.m3e-switch.is-checked.is-danger .m3e-switch__thumb {
  background-color: var(--md-sys-color-on-error);
  color: var(--md-sys-color-error);
}

/* Pressed thumb: expands to 28px */
.m3e-switch.is-pressed:not(:disabled) .m3e-switch__thumb {
  width: 28px;
  height: 28px;
}

/* Icon inside thumb with rotation and scale spring morph */
.m3e-switch__icon {
  width: 16px;
  height: 16px;
  display: block;
  transition:
    transform 250ms cubic-bezier(0.38, 1.21, 0.22, 1),
    color 200ms ease;
}

.m3e-switch__icon.is-checked-icon {
  transform: rotate(0deg) scale(1);
}

.m3e-switch__icon.is-unchecked-icon {
  transform: rotate(-90deg) scale(0.85);
}

/* Disabled = 38% content, 12% container */
.m3e-switch:disabled,
.m3e-switch.is-disabled {
  cursor: not-allowed;
  pointer-events: none;
}

.m3e-switch.is-disabled .m3e-switch__track {
  background-color: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent);
  border-color: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent);
}

.m3e-switch.is-disabled .m3e-switch__thumb {
  background-color: color-mix(in srgb, var(--md-sys-color-on-surface) 38%, transparent);
  color: color-mix(in srgb, var(--md-sys-color-surface) 100%, transparent);
}

/* Respect prefers-reduced-motion: no morph, just color change */
@media (prefers-reduced-motion: reduce) {
  .m3e-switch__track,
  .m3e-switch__handle-container,
  .m3e-switch__thumb {
    transition:
      background-color 150ms ease,
      border-color 150ms ease,
      color 150ms ease !important;
  }
}
</style>
