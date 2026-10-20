<script setup lang="ts">
interface Props {
  variant?: 'filled' | 'elevated' | 'tonal' | 'outlined' | 'text'
  size?: 'xs' | 's' | 'm' | 'l' | 'xl'
  icon?: string
  trailingIcon?: string
  disabled?: boolean
  loading?: boolean
  type?: 'button' | 'submit' | 'reset'
  ariaLabel?: string
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'filled',
  size: 's',
  icon: undefined,
  trailingIcon: undefined,
  disabled: false,
  loading: false,
  type: 'button',
  ariaLabel: undefined,
})

const emit = defineEmits<{
  (e: 'click', event: MouseEvent): void
}>()

function onClick(e: MouseEvent) {
  if (props.disabled || props.loading) {
    e.stopPropagation()
    e.preventDefault()
    return
  }
  emit('click', e)
}
</script>

<template>
  <md-button
    :variant="variant"
    :size="size"
    :icon="loading ? 'progress_activity' : icon"
    :trailing-icon="trailingIcon"
    :disabled="disabled || loading || undefined"
    :type="type"
    :aria-label="ariaLabel"
    class="m3e-button select-none cursor-pointer"
    :class="loading && 'is-loading'"
    @click="onClick"
  >
    <slot />
  </md-button>
</template>

<style scoped>
.m3e-button {
  display: inline-block;
  vertical-align: middle;
}
.m3e-button.is-loading {
  pointer-events: none;
  opacity: 0.85;
}
</style>
