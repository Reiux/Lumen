<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    name: string
    filled?: boolean
    size?: number | string
  }>(),
  {
    filled: false,
    size: 24,
  }
)

const sizeStyle = computed(() => {
  const s = typeof props.size === 'number' ? `${props.size}px` : props.size
  const numSize = typeof props.size === 'number' ? props.size : parseInt(props.size, 10) || 24
  const opsz = Math.min(48, Math.max(20, numSize))
  return {
    fontSize: s,
    width: s,
    height: s,
    fontVariationSettings: `'FILL' ${props.filled ? 1 : 0}, 'wght' 400, 'GRAD' 0, 'opsz' ${opsz}`,
  }
})
</script>

<template>
  <span
    class="m-icon inline-flex items-center justify-center shrink-0 leading-none select-none"
    :class="{ 'm-icon-filled': filled }"
    :style="sizeStyle"
    aria-hidden="true"
  >{{ name }}</span>
</template>
