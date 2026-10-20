<script setup lang="ts">
interface Props {
  disabled?: boolean
  loading?: boolean
}

withDefaults(defineProps<Props>(), {
  disabled: false,
  loading: false,
})

const emit = defineEmits<{
  (e: 'pick-files'): void
  (e: 'pick-folder'): void
}>()
</script>

<template>
  <div class="m3e-fab-container select-none">
    <md-button-group
      variant="connected"
      aria-label="Upload picker actions"
      class="m3e-fab-group"
    >
      <!-- Primary: Choose Files -->
      <md-button
        variant="filled"
        size="m"
        icon="add_photo_alternate"
        :disabled="disabled || loading || undefined"
        aria-label="Choose files to upload"
        @click="emit('pick-files')"
      >
        Choose Files
      </md-button>

      <!-- Secondary: Choose Folder -->
      <md-button
        variant="tonal"
        size="m"
        icon="folder"
        :disabled="disabled || loading || undefined"
        aria-label="Choose folder to upload"
        @click="emit('pick-folder')"
      >
        Folder
      </md-button>
    </md-button-group>
  </div>
</template>

<style scoped>
.m3e-fab-container {
  display: inline-flex;
  align-items: center;
  filter: drop-shadow(0 8px 18px oklch(0.70 0.22 250 / 25%));
  transition: transform var(--md-sys-motion-duration-spatial-fast) var(--md-sys-motion-easing-spatial);
}
.m3e-fab-container:hover {
  transform: translateY(-2px);
}
@media (prefers-reduced-motion: reduce) {
  .m3e-fab-container {
    transition: none !important;
    transform: none !important;
  }
}
</style>
