<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'

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

const switchRef = ref<HTMLElement | null>(null)

function onChange(e: Event) {
  if (props.disabled) return
  const target = e.target as { checked?: boolean }
  const isChecked = target?.checked ?? !props.modelValue
  emit('update:modelValue', isChecked)
  emit('change', isChecked)
}

watch(
  () => props.modelValue,
  (val) => {
    if (switchRef.value) {
      ;(switchRef.value as unknown as { checked: boolean }).checked = val
    }
  }
)

onMounted(() => {
  if (switchRef.value) {
    ;(switchRef.value as unknown as { checked: boolean }).checked = props.modelValue
  }
})
</script>

<template>
  <md-switch
    :id="id"
    ref="switchRef"
    :checked="modelValue || undefined"
    :disabled="disabled || undefined"
    :aria-label="ariaLabel"
    class="m3e-switch cursor-pointer select-none"
    :class="variant === 'danger' && 'm3e-switch-danger'"
    @change="onChange"
  />
</template>

<style scoped>
.m3e-switch {
  display: inline-block;
  vertical-align: middle;
}
.m3e-switch-danger {
  --md-sys-color-primary: var(--md-sys-color-error);
  --md-sys-color-on-primary: var(--md-sys-color-on-error);
}
</style>
