<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  size?: 'xs' | 'sm' | 'md' | 'lg' | number
  color?: string
  label?: string
}

const props = withDefaults(defineProps<Props>(), {
  size: 'md',
  color: 'var(--md-sys-color-primary)',
  label: 'Loading...',
})

const sizePx = computed(() => {
  if (typeof props.size === 'number') return `${props.size}px`
  switch (props.size) {
    case 'xs': return '16px'
    case 'sm': return '22px'
    case 'lg': return '52px'
    case 'md':
    default: return '32px'
  }
})
</script>

<template>
  <div
    class="m3-loading-indicator inline-flex items-center justify-center shrink-0 select-none"
    role="status"
    :aria-label="label"
    :style="{ width: sizePx, height: sizePx }"
  >
    <!-- Outer Morphing Shape -->
    <div
      class="m3-morph-shape absolute inset-0"
      :style="{ borderColor: color }"
    />
    <!-- Inner Expressive Pulse Core -->
    <div
      class="m3-morph-core size-1/3 rounded-full"
      :style="{ backgroundColor: color }"
    />
    <span class="sr-only">{{ label }}</span>
  </div>
</template>

<style scoped>
.m3-loading-indicator {
  position: relative;
  isolation: isolate;
}

.m3-morph-shape {
  border-width: 2.5px;
  border-style: solid;
  border-radius: 50%;
  animation: m3-shape-morph-spin 2.6s cubic-bezier(0.4, 0, 0.2, 1) infinite;
  box-sizing: border-box;
}

.m3-morph-core {
  animation: m3-core-pulse 1.3s cubic-bezier(0.34, 1.56, 0.64, 1) infinite alternate;
}

@keyframes m3-shape-morph-spin {
  0% {
    border-radius: 50%;
    transform: rotate(0deg) scale(0.92);
  }
  25% {
    border-radius: 36% 14% 36% 14%;
    transform: rotate(90deg) scale(1.06);
  }
  50% {
    border-radius: 28%;
    transform: rotate(180deg) scale(0.92);
  }
  75% {
    border-radius: 14% 36% 14% 36%;
    transform: rotate(270deg) scale(1.06);
  }
  100% {
    border-radius: 50%;
    transform: rotate(360deg) scale(0.92);
  }
}

@keyframes m3-core-pulse {
  0% {
    transform: scale(0.65);
    opacity: 0.45;
  }
  100% {
    transform: scale(1.15);
    opacity: 1;
  }
}

@media (prefers-reduced-motion: reduce) {
  .m3-morph-shape {
    animation: none;
    border-radius: 50%;
    transform: none;
  }
  .m3-morph-core {
    animation: m3-core-reduced 1.5s ease-in-out infinite alternate;
  }
}

@keyframes m3-core-reduced {
  0% { opacity: 0.4; }
  100% { opacity: 1; }
}
</style>
