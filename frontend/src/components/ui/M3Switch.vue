<script setup lang="ts">
import M3Icon from './M3Icon.vue'

interface Props {
  modelValue: boolean
  disabled?: boolean
  variant?: 'default' | 'danger'
  id?: string
  ariaLabel?: string
}

const props = withDefaults(defineProps<Props>(), {
  disabled: false,
  variant: 'default',
  id: undefined,
  ariaLabel: undefined,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'change', value: boolean): void
}>()

function toggle() {
  if (props.disabled) return
  const next = !props.modelValue
  emit('update:modelValue', next)
  emit('change', next)
}
</script>

<template>
  <button
    :id="id"
    type="button"
    role="switch"
    :aria-checked="modelValue"
    :aria-label="ariaLabel"
    :disabled="disabled"
    class="group m3-switch no-state-layer relative inline-flex h-8 w-[52px] shrink-0 cursor-pointer items-center rounded-full border-2 p-0.5 outline-none disabled:cursor-not-allowed disabled:opacity-40 select-none"
    :class="[
      modelValue
        ? (variant === 'danger'
          ? 'bg-[var(--md-sys-color-error)] border-[var(--md-sys-color-error)]'
          : 'bg-[var(--md-sys-color-primary)] border-[var(--md-sys-color-primary)]')
        : 'bg-[var(--md-sys-color-surface-container-highest)] border-[var(--md-sys-color-outline)]',
    ]"
    @click="toggle"
  >
    <span
      class="m3-switch-thumb flex h-6 items-center justify-center rounded-full shadow-xs"
      :class="[
        modelValue
          ? (variant === 'danger'
            ? 'w-6 translate-x-[20px] bg-[var(--md-sys-color-on-error)] text-[var(--md-sys-color-error)] group-active:w-7 group-active:translate-x-[16px]'
            : 'w-6 translate-x-[20px] bg-[var(--md-sys-color-on-primary)] text-[var(--md-sys-color-primary)] group-active:w-7 group-active:translate-x-[16px]')
          : 'w-4 translate-x-[4px] bg-[var(--md-sys-color-outline)] text-transparent group-active:w-6',
      ]"
    >
      <M3Icon
        name="check"
        :size="14"
        class="font-bold m3-switch-icon"
        :class="modelValue ? 'opacity-100 scale-100' : 'opacity-0 scale-50'"
      />
    </span>
  </button>
</template>

<style scoped>
.m3-switch {
  border-radius: 9999px !important;
  overflow: hidden;
  -webkit-tap-highlight-color: transparent !important;
  outline: none !important;
  transition: background-color var(--md-sys-motion-effects),
              border-color var(--md-sys-motion-effects);
}

.m3-switch:active {
  border-radius: 9999px !important;
  transform: none !important;
}

.m3-switch-thumb {
  transition: transform var(--md-sys-motion-spatial),
              width var(--md-sys-motion-spatial-fast),
              background-color var(--md-sys-motion-effects),
              color var(--md-sys-motion-effects);
}

.m3-switch-icon {
  transition: opacity var(--md-sys-motion-effects),
              transform var(--md-sys-motion-effects);
}
</style>
