<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'

interface Props {
  value?: number
  max?: number
  indeterminate?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  value: 0,
  max: 100,
  indeterminate: false,
})

const progRef = ref<HTMLElement | null>(null)

watch(
  () => props.value,
  (val) => {
    if (progRef.value) {
      ;(progRef.value as unknown as { value: number }).value = val
    }
  }
)

onMounted(() => {
  if (progRef.value) {
    ;(progRef.value as unknown as { value: number }).value = props.value
  }
})
</script>

<template>
  <div class="m3e-wavy-wrapper w-full overflow-hidden select-none">
    <md-progress-indicator
      ref="progRef"
      type="linear"
      variant="wavy"
      :value="value"
      :max="max"
      :indeterminate="indeterminate || undefined"
      class="m3e-progress w-full"
    />
  </div>
</template>

<style scoped>
.m3e-wavy-wrapper {
  display: block;
  min-height: 12px;
}
.m3e-progress {
  display: block;
  width: 100%;
}
</style>
