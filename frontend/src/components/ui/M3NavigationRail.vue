<script setup lang="ts">
import { computed } from 'vue'

export interface NavRailItem {
  id: string
  icon: string
  label: string
  ariaLabel?: string
}

interface Props {
  modelValue: string
  items: NavRailItem[]
  expanded?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  expanded: false,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
}>()

const selectedIndex = computed(() => {
  const idx = props.items.findIndex((item) => item.id === props.modelValue)
  return idx >= 0 ? idx : 0
})

const itemsJson = computed(() => {
  return JSON.stringify(
    props.items.map((item) => ({
      icon: item.icon,
      label: item.label,
      ariaLabel: item.ariaLabel || item.label,
    }))
  )
})

function onChange(e: Event) {
  const customEvent = e as CustomEvent<{ index: number }>
  const index = customEvent.detail?.index ?? 0
  const matched = props.items[index]
  if (matched && matched.id !== props.modelValue) {
    emit('update:modelValue', matched.id)
  }
}
</script>

<template>
  <md-navigation-rail
    :expanded="expanded || undefined"
    :selected="selectedIndex"
    :items="itemsJson"
    class="m3e-nav-rail shrink-0 select-none border-r border-[var(--md-sys-color-outline-variant)]/30"
    style="--rail-container: var(--md-sys-color-surface-container-low); --wails-draggable: none;"
    @change="onChange"
  >
    <div
      v-if="$slots.header"
      slot="header"
      class="m3e-rail-header w-full"
    >
      <slot name="header" />
    </div>

    <div
      v-if="$slots.default"
      class="m3e-rail-footer w-full p-2"
    >
      <slot />
    </div>
  </md-navigation-rail>
</template>

<style scoped>
.m3e-nav-rail {
  display: inline-block;
  height: 100%;
  vertical-align: top;
  contain: layout style;
}
.m3e-rail-header {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
}
.m3e-rail-footer {
  display: flex;
  flex-direction: column;
  align-items: center;
  margin-top: auto;
}
</style>
