<script setup lang="ts">
import { Check } from '@lucide/vue'

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
    class="relative inline-flex h-8 w-[52px] shrink-0 cursor-pointer items-center rounded-full p-1 transition-colors duration-200 outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)] disabled:cursor-not-allowed disabled:opacity-40 select-none"
    :class="[
      modelValue
        ? (variant === 'danger'
          ? 'bg-[var(--md-sys-color-error)] border-2 border-[var(--md-sys-color-error)]'
          : 'bg-[var(--md-sys-color-primary)] border-2 border-[var(--md-sys-color-primary)]')
        : 'bg-[var(--md-sys-color-surface-container-highest)] border-2 border-[var(--md-sys-color-outline)]'
    ]"
    @click="toggle"
  >
    <span
      class="flex size-6 items-center justify-center rounded-full transition-transform duration-200 shadow-xs"
      :class="[
        modelValue
          ? (variant === 'danger'
            ? 'translate-x-4 bg-[var(--md-sys-color-on-error)] text-[var(--md-sys-color-error)]'
            : 'translate-x-4 bg-[var(--md-sys-color-on-primary)] text-[var(--md-sys-color-primary)]')
          : 'translate-x-0 bg-[var(--md-sys-color-outline)] text-transparent scale-75'
      ]"
    >
      <Check
        v-if="modelValue"
        class="size-3.5 stroke-[3]"
      />
    </span>
  </button>
</template>
