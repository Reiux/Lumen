<script setup lang="ts">
import { computed, ref } from 'vue'
import M3Icon from './M3Icon.vue'

interface Props {
  variant?: 'filled' | 'elevated' | 'tonal' | 'outlined' | 'text'
  size?: 'xs' | 's' | 'm' | 'l' | 'xl'
  color?: 'primary' | 'error' | 'secondary'
  destructive?: boolean
  selected?: boolean
  toggle?: boolean
  icon?: string
  trailingIcon?: string
  disabled?: boolean
  loading?: boolean
  type?: 'button' | 'submit' | 'reset'
  ariaLabel?: string
  title?: string
  fullWidth?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'filled',
  size: 's',
  color: 'primary',
  destructive: false,
  selected: undefined,
  toggle: false,
  icon: undefined,
  trailingIcon: undefined,
  disabled: false,
  loading: false,
  type: 'button',
  ariaLabel: undefined,
  title: undefined,
  fullWidth: false,
})

const emit = defineEmits<{
  (e: 'click', event: MouseEvent): void
}>()

const isPressed = ref(false)

const isSelected = computed(() => {
  if (props.selected !== undefined) return props.selected
  return false
})

const effectiveVariant = computed(() => {
  if (props.toggle && isSelected.value) {
    return 'tonal'
  }
  return props.variant
})

let pressTimeout: ReturnType<typeof setTimeout> | null = null

function onClick(e: MouseEvent) {
  if (props.disabled || props.loading) {
    e.stopPropagation()
    e.preventDefault()
    return
  }
  emit('click', e)
}

function onPointerDown() {
  if (props.disabled || props.loading) return
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
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    :aria-label="ariaLabel"
    :aria-pressed="toggle ? isSelected : undefined"
    :title="title"
    class="m3e-btn select-none cursor-pointer"
    :class="[
      `m3e-btn--${effectiveVariant}`,
      `m3e-btn--${size}`,
      (destructive || color === 'error') && 'm3e-btn--destructive',
      color === 'secondary' && 'm3e-btn--secondary',
      toggle && 'm3e-btn--toggle',
      isSelected && 'm3e-btn--selected',
      isPressed && 'm3e-btn--pressed',
      loading && 'is-loading',
      fullWidth && 'w-full',
      disabled && 'is-disabled',
    ]"
    @click="onClick"
    @pointerdown="onPointerDown"
    @pointerup="onPointerUp"
    @pointerleave="onPointerLeave"
    @pointercancel="onPointerLeave"
  >
    <!-- State Layer overlay -->
    <span class="m3e-state-layer" aria-hidden="true" />

    <!-- Leading Icon / Spinner -->
    <span
      v-if="loading || icon || $slots.icon"
      class="m3e-btn__icon leading-none flex items-center justify-center shrink-0"
    >
      <slot name="icon">
        <M3Icon
          v-if="loading"
          name="progress_activity"
          :size="18"
          class="animate-spin"
        />
        <M3Icon
          v-else-if="icon"
          :name="icon"
          :size="18"
        />
      </slot>
    </span>

    <!-- Label -->
    <span v-if="$slots.default" class="m3e-btn__label truncate">
      <slot />
    </span>

    <!-- Trailing Icon -->
    <span
      v-if="trailingIcon || $slots.trailingIcon"
      class="m3e-btn__trailing-icon leading-none flex items-center justify-center shrink-0"
    >
      <slot name="trailingIcon">
        <M3Icon
          :name="trailingIcon!"
          :size="18"
        />
      </slot>
    </span>
  </button>
</template>

<style scoped>
.m3e-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  vertical-align: middle;
  white-space: nowrap;
  flex-shrink: 0;
  gap: 8px;
  border-radius: 28px;
  border: 1px solid transparent;
  outline: none;
  font-family: inherit;
  font-size: var(--m3-type-label-large-size);
  line-height: var(--m3-type-label-large-line-height);
  font-weight: var(--m3-type-label-large-weight);
  letter-spacing: var(--m3-type-label-large-letter-spacing);
  overflow: hidden;
  box-sizing: border-box;
  text-decoration: none;
  -webkit-tap-highlight-color: transparent;
  transition:
    border-radius 180ms cubic-bezier(0.2, 0, 0, 1),
    transform 200ms cubic-bezier(0.38, 1.21, 0.22, 1),
    background-color 150ms ease,
    border-color 150ms ease,
    color 150ms ease,
    box-shadow 150ms ease;
}

/* Morph on press: from full-round (28px) to squarer squircle (14px) and 0.97 scale with recoil */
.m3e-btn:active:not(:disabled),
.m3e-btn--pressed:not(:disabled) {
  border-radius: 14px;
  transform: scale(0.97);
}

/* Toggle buttons: selected = rounded-square (~14px) with tonal/primary fill */
.m3e-btn--toggle.m3e-btn--selected {
  border-radius: 14px;
  background-color: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.m3e-btn--toggle:not(.m3e-btn--selected) {
  border-radius: 28px;
}

/* Sizes: S (40px), M (48px default for primary page actions), L (56px) */
.m3e-btn--xs {
  height: 32px;
  padding: 0 12px;
  font-size: var(--m3-type-label-small-size);
  line-height: var(--m3-type-label-small-line-height);
  font-weight: var(--m3-type-label-small-weight);
  gap: 6px;
}

.m3e-btn--s {
  height: 40px;
  padding: 0 16px;
  font-size: var(--m3-type-label-large-size);
  line-height: var(--m3-type-label-large-line-height);
  font-weight: var(--m3-type-label-large-weight);
  letter-spacing: var(--m3-type-label-large-letter-spacing);
  gap: 8px;
}

.m3e-btn--m {
  height: 48px;
  padding: 0 24px;
  font-size: var(--m3-type-label-large-size);
  line-height: var(--m3-type-label-large-line-height);
  font-weight: var(--m3-type-label-large-weight);
  letter-spacing: var(--m3-type-label-large-letter-spacing);
  gap: 8px;
}

.m3e-btn--l {
  height: 56px;
  padding: 0 32px;
  font-size: var(--m3-type-title-medium-size);
  line-height: var(--m3-type-title-medium-line-height);
  font-weight: var(--m3-type-title-medium-weight);
  gap: 10px;
}

.m3e-btn--xl {
  height: 64px;
  padding: 0 36px;
  font-size: var(--m3-type-headline-small-size);
  line-height: var(--m3-type-headline-small-line-height);
  font-weight: var(--m3-type-headline-small-weight);
  gap: 12px;
}

/* Icon 18px */
.m3e-btn__icon :deep(svg),
.m3e-btn__trailing-icon :deep(svg) {
  width: 18px;
  height: 18px;
}

/* State layers: hover 8%, focus 10%, pressed 10% overlay of content color */
.m3e-state-layer {
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background-color: currentColor;
  opacity: 0;
  pointer-events: none;
  transition: opacity 150ms ease;
}

.m3e-btn:hover:not(:disabled) > .m3e-state-layer {
  opacity: 0.08;
}

.m3e-btn:focus-visible > .m3e-state-layer {
  opacity: 0.10;
}

.m3e-btn:active:not(:disabled) > .m3e-state-layer,
.m3e-btn--pressed:not(:disabled) > .m3e-state-layer {
  opacity: 0.10;
}

/* Keyboard focus ring (3px, primary) */
.m3e-btn:focus-visible {
  outline: 3px solid var(--md-sys-color-primary);
  outline-offset: 2px;
}

/* Variants */
.m3e-btn--filled {
  background-color: var(--md-sys-color-primary);
  color: var(--md-sys-color-on-primary);
  border-color: transparent;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
}

.m3e-btn--tonal {
  background-color: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
  border-color: transparent;
}

.m3e-btn--elevated {
  background-color: var(--md-sys-color-surface-container-low);
  color: var(--md-sys-color-primary);
  border-color: transparent;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12), 0 1px 2px rgba(0, 0, 0, 0.18);
}

.m3e-btn--elevated:hover:not(:disabled) {
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.16), 0 1px 2px rgba(0, 0, 0, 0.22);
}

.m3e-btn--outlined {
  background-color: transparent;
  color: var(--md-sys-color-primary);
  border-color: var(--md-sys-color-outline);
}

.m3e-btn--text {
  background-color: transparent;
  color: var(--md-sys-color-primary);
  border-color: transparent;
  padding-left: 12px;
  padding-right: 12px;
}

/* Secondary color variants */
.m3e-btn--secondary.m3e-btn--filled {
  background-color: var(--md-sys-color-secondary);
  color: var(--md-sys-color-on-secondary);
}

.m3e-btn--secondary.m3e-btn--tonal {
  background-color: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.m3e-btn--secondary.m3e-btn--outlined {
  border-color: var(--md-sys-color-secondary);
  color: var(--md-sys-color-secondary);
}

.m3e-btn--secondary.m3e-btn--text {
  color: var(--md-sys-color-secondary);
}

/* Destructive / Error variants */
.m3e-btn--destructive.m3e-btn--filled {
  background-color: var(--md-sys-color-error);
  color: var(--md-sys-color-on-error);
}

.m3e-btn--destructive.m3e-btn--tonal {
  background-color: var(--md-sys-color-error-container);
  color: var(--md-sys-color-on-error-container);
}

.m3e-btn--destructive.m3e-btn--outlined {
  border-color: var(--md-sys-color-error);
  color: var(--md-sys-color-error);
}

.m3e-btn--destructive.m3e-btn--text {
  color: var(--md-sys-color-error);
}

/* Disabled = 38% content, 12% container */
.m3e-btn:disabled,
.m3e-btn.is-disabled {
  cursor: not-allowed;
  pointer-events: none;
  box-shadow: none !important;
}

.m3e-btn--filled:disabled,
.m3e-btn--tonal:disabled,
.m3e-btn--elevated:disabled {
  background-color: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent);
  color: color-mix(in srgb, var(--md-sys-color-on-surface) 38%, transparent);
  border-color: transparent;
}

.m3e-btn--outlined:disabled {
  background-color: transparent;
  border-color: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent);
  color: color-mix(in srgb, var(--md-sys-color-on-surface) 38%, transparent);
}

.m3e-btn--text:disabled {
  background-color: transparent;
  border-color: transparent;
  color: color-mix(in srgb, var(--md-sys-color-on-surface) 38%, transparent);
}

/* Loading */
.m3e-btn.is-loading {
  pointer-events: none;
  opacity: 0.85;
}

/* Reduced motion preference: no morph, just color change */
@media (prefers-reduced-motion: reduce) {
  .m3e-btn {
    transition:
      background-color 150ms ease,
      border-color 150ms ease,
      color 150ms ease !important;
  }
  .m3e-btn:active:not(:disabled),
  .m3e-btn--pressed:not(:disabled) {
    border-radius: 9999px !important;
  }
  .m3e-btn--toggle.m3e-btn--selected {
    border-radius: 14px !important;
  }
}
</style>
