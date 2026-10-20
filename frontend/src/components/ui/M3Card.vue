<script setup lang="ts">
interface Props {
  variant?: 'elevated' | 'filled' | 'outlined'
  interactive?: boolean
  disabled?: boolean
}

withDefaults(defineProps<Props>(), {
  variant: 'filled',
  interactive: false,
  disabled: false,
})

const emit = defineEmits<{
  (e: 'click', event: MouseEvent): void
}>()
</script>

<template>
  <md-card
    :variant="variant"
    :interactive="interactive || undefined"
    :disabled="disabled || undefined"
    class="m3e-card w-full"
    @click="emit('click', $event)"
  >
    <slot name="header" />
    <slot />
    <slot name="actions" />
  </md-card>
</template>

<style scoped>
.m3e-card {
  display: block;
}
</style>
