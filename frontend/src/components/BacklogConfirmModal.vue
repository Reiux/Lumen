<script setup lang="ts">
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'

defineProps<{
  pendingBacklog: { folder: string; count: number; account: string } | null
}>()

const emit = defineEmits<{
  (e: 'cancel'): void
  (e: 'confirm', uploadExisting: boolean): void
}>()
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="pendingBacklog"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
        @click.self="emit('cancel')"
      >
        <div
          class="m3-dialog-card relative flex flex-col gap-4 w-full max-w-md rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl border border-[var(--md-sys-color-outline-variant)]/40"
          role="dialog"
          aria-modal="true"
          aria-labelledby="backlog-confirm-title"
        >
          <div class="flex items-center gap-3 text-[var(--md-sys-color-primary)]">
            <div class="flex size-10 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-primary-container)]">
              <M3Icon
                name="folder"
                :filled="true"
                :size="22"
              />
            </div>
            <h3
              id="backlog-confirm-title"
              class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
            >
              Upload Existing Backlog?
            </h3>
          </div>
          <p class="m3-body-medium leading-relaxed">
            Found <strong class="text-[var(--md-sys-color-on-surface)] m3-title-medium">{{ pendingBacklog.count }}</strong> existing photo and video files in:
          </p>
          <div class="rounded-xl bg-[var(--md-sys-color-surface-container)] p-3 m3-mono m3-body-medium break-all border border-[var(--md-sys-color-outline-variant)]/30 text-[var(--md-sys-color-on-surface)]">
            {{ pendingBacklog.folder }}
          </div>
          <p class="m3-body-medium leading-relaxed">
            Would you like to upload existing files now, or only monitor this folder for newly added files in the future?
          </p>
          <div class="flex flex-col sm:flex-row items-center justify-end gap-2 pt-2">
            <M3Button
              variant="text"
              size="s"
              class="w-full sm:w-auto"
              @click="emit('cancel')"
            >
              Cancel
            </M3Button>
            <M3Button
              variant="tonal"
              size="s"
              class="w-full sm:w-auto"
              @click="emit('confirm', false)"
            >
              Watch Only
            </M3Button>
            <M3Button
              variant="filled"
              size="s"
              class="w-full sm:w-auto"
              @click="emit('confirm', true)"
            >
              Upload Backlog & Watch
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
