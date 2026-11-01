<script setup lang="ts">
interface Props {
  connected?: boolean
}

withDefaults(defineProps<Props>(), {
  connected: false,
})
</script>

<template>
  <div
    class="m3e-button-group inline-flex items-center select-none"
    :class="connected ? 'm3e-button-group--connected' : 'gap-3'"
    role="group"
  >
    <slot />
  </div>
</template>

<style scoped>
.m3e-button-group {
  box-sizing: border-box;
}

.m3e-button-group--connected {
  border-radius: 9999px;
  overflow: hidden;
  border: 1px solid var(--md-sys-color-outline-variant);
  background-color: var(--md-sys-color-surface-container);
}

.m3e-button-group--connected :deep(.m3e-btn) {
  border-radius: 0 !important;
  border: none;
}

.m3e-button-group--connected :deep(.m3e-btn:first-child) {
  border-top-left-radius: 9999px !important;
  border-bottom-left-radius: 9999px !important;
}

.m3e-button-group--connected :deep(.m3e-btn:last-child) {
  border-top-right-radius: 9999px !important;
  border-bottom-right-radius: 9999px !important;
}

.m3e-button-group--connected :deep(.m3e-btn:not(:last-child)) {
  border-right: 1px solid var(--md-sys-color-outline-variant);
}
</style>
