<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  size?: 'xs' | 'sm' | 'md' | 'lg' | number
  color?: string
  label?: string
  contained?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  size: 'md',
  color: undefined,
  label: 'Loading...',
  contained: false,
})

const scale = computed(() => {
  if (typeof props.size === 'number') {
    return props.size / 48
  }
  switch (props.size) {
    case 'xs': return 0.45
    case 'sm': return 0.6
    case 'lg': return 1.0
    case 'md':
    default: return 0.75
  }
})

const containerSize = computed(() => {
  if (typeof props.size === 'number') return `${props.size}px`
  switch (props.size) {
    case 'xs': return '22px'
    case 'sm': return '28px'
    case 'lg': return '48px'
    case 'md':
    default: return '36px'
  }
})
</script>

<template>
  <div
    class="m3e-loading-container inline-flex items-center justify-center shrink-0 select-none"
    role="status"
    :aria-label="label"
    :style="{ width: containerSize, height: containerSize }"
  >
    <div
      class="m3e-loading-scaler origin-center"
      :style="{ transform: `scale(${scale})` }"
    >
      <md-loading-indicator
        :variant="contained ? 'contained' : undefined"
        :aria-label="label"
        class="m3e-loading-element"
      />
    </div>
    <span class="sr-only">{{ label }}</span>
  </div>
</template>

<style scoped>
.m3e-loading-container {
  overflow: hidden;
  vertical-align: middle;
}
.m3e-loading-scaler {
  width: 48px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.m3e-loading-element {
  display: block;
}
</style>
