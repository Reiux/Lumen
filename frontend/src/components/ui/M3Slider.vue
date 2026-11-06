<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'

interface Props {
  modelValue: number
  min?: number
  max?: number
  step?: number
  stops?: boolean
  labeled?: boolean
  disabled?: boolean
  id?: string
  ariaLabel?: string
}

const props = withDefaults(defineProps<Props>(), {
  min: 0,
  max: 100,
  step: 1,
  stops: false,
  labeled: true,
  disabled: false,
  id: undefined,
  ariaLabel: undefined,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: number): void
  (e: 'change', value: number): void
}>()

const sliderRef = ref<HTMLElement | null>(null)

function onInput(e: Event) {
  const customEvent = e as CustomEvent<{ value: number }>
  const val = customEvent.detail?.value ?? Number((e.target as unknown as { value: number })?.value)
  emit('update:modelValue', val)
}

function onChange(e: Event) {
  const customEvent = e as CustomEvent<{ value: number }>
  const val = customEvent.detail?.value ?? Number((e.target as unknown as { value: number })?.value)
  emit('update:modelValue', val)
  emit('change', val)
}

watch(
  () => props.modelValue,
  (val) => {
    if (sliderRef.value) {
      ;(sliderRef.value as unknown as { value: number }).value = val
    }
  }
)

onMounted(() => {
  if (sliderRef.value) {
    ;(sliderRef.value as unknown as { value: number }).value = props.modelValue
  }
})
</script>

<template>
  <md-slider
    :id="id"
    ref="sliderRef"
    :value="modelValue"
    :min="min"
    :max="max"
    :step="step"
    :stops="stops || undefined"
    :labeled="labeled || undefined"
    :disabled="disabled || undefined"
    :aria-label="ariaLabel"
    class="m3e-slider w-full cursor-pointer select-none"
    @input="onInput"
    @change="onChange"
  />
</template>

<style scoped>
.m3e-slider {
  display: block;
}
</style>
