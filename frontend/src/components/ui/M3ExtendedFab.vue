<script setup lang="ts">
import Button from './button/Button.vue'
import M3Icon from './M3Icon.vue'

withDefaults(
  defineProps<{
    disabled?: boolean
    loading?: boolean
  }>(),
  {
    disabled: false,
    loading: false,
  }
)

const emit = defineEmits<{
  (e: 'pick-files'): void
  (e: 'pick-folder'): void
}>()
</script>

<template>
  <div
    class="m3-extended-fab-group m3-button-group shadow-lg select-none"
    role="group"
    aria-label="Upload picker actions"
  >
    <!-- Choose Files Primary Action -->
    <Button
      type="button"
      class="m3-btn-group-start h-12 md:h-14 px-5 text-xs md:text-sm font-bold tracking-wide cursor-pointer select-none bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)] hover:bg-[var(--md-sys-color-primary)] hover:text-[var(--md-sys-color-on-primary)] border border-[var(--md-sys-color-primary)]/30 active:scale-[0.98]"
      :disabled="disabled || loading"
      aria-label="Choose files to upload"
      @click="emit('pick-files')"
    >
      <M3Icon
        name="add_photo_alternate"
        :filled="true"
        :size="20"
        class="mr-2"
      />
      <span>Choose Files</span>
    </Button>

    <!-- Choose Folder Secondary Action -->
    <Button
      type="button"
      class="m3-btn-group-end h-12 md:h-14 px-4 text-xs md:text-sm font-semibold tracking-wide cursor-pointer select-none bg-[var(--md-sys-color-primary-container)]/80 text-[var(--md-sys-color-on-primary-container)] hover:bg-[var(--md-sys-color-primary)] hover:text-[var(--md-sys-color-on-primary)] border border-[var(--md-sys-color-primary)]/30 active:scale-[0.98]"
      :disabled="disabled || loading"
      aria-label="Choose folder to upload"
      @click="emit('pick-folder')"
    >
      <M3Icon
        name="create_new_folder"
        :filled="true"
        :size="20"
        class="mr-1.5"
      />
      <span>Folder</span>
    </Button>
  </div>
</template>

<style scoped>
.m3-extended-fab-group {
  position: relative;
  isolation: isolate;
  filter: drop-shadow(0 8px 16px oklch(0.70 0.22 250 / 22%));
  transition: transform var(--md-sys-motion-spatial-fast),
              filter var(--md-sys-motion-effects);
}

.m3-extended-fab-group:hover {
  filter: drop-shadow(0 12px 24px oklch(0.70 0.22 250 / 32%));
}

@media (prefers-reduced-motion: reduce) {
  .m3-extended-fab-group {
    transition: none !important;
  }
}
</style>
