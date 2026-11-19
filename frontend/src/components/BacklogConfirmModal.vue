<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

const { t } = useI18n()

const props = defineProps<{
  pendingBacklog: { folder: string; count: number; account: string } | null
  loading?: boolean
}>()

const emit = defineEmits<{
  (e: 'cancel'): void
  (e: 'confirm', uploadExisting: boolean): void
}>()

const dialogCardRef = ref<HTMLElement | null>(null)
useDialogFocusTrap(() => Boolean(props.pendingBacklog), dialogCardRef, {
  onEscape: () => {
    if (!props.loading) emit('cancel')
  },
})
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="pendingBacklog"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
        @click.self="!loading && emit('cancel')"
      >
        <div
          ref="dialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex flex-col gap-5 w-full max-w-[460px] rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
          role="dialog"
          aria-modal="true"
          aria-labelledby="backlog-confirm-title"
          aria-describedby="backlog-confirm-desc"
        >
          <!-- M3E Centered Hero Icon + Headline -->
          <div class="flex flex-col items-center text-center gap-3">
            <div class="flex size-14 items-center justify-center rounded-[20px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
              <M3Icon
                name="folder_copy"
                :filled="true"
                :size="28"
              />
            </div>
            <div class="flex flex-col gap-1.5">
              <h3
                id="backlog-confirm-title"
                class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
              >
                {{ t('dialogs.backlog.title') }}
              </h3>
              <p
                id="backlog-confirm-desc"
                class="m3-body-medium leading-relaxed text-[var(--md-sys-color-on-surface-variant)]"
              >
                {{ t('dialogs.backlog.found', { count: pendingBacklog.count }) }}
              </p>
            </div>
          </div>

          <!-- Tonal Folder Path Surface -->
          <div class="flex items-start gap-3 rounded-[16px] bg-[var(--md-sys-color-surface-container)] px-4 py-3.5 text-[var(--md-sys-color-on-surface)]">
            <div class="flex size-9 shrink-0 items-center justify-center rounded-[12px] bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]">
              <M3Icon
                name="folder_open"
                :filled="true"
                :size="18"
              />
            </div>
            <span class="m3-mono m3-body-medium break-all leading-snug pt-1 text-[var(--md-sys-color-on-surface)]">
              {{ pendingBacklog.folder }}
            </span>
          </div>

          <p class="m3-body-medium leading-relaxed text-[var(--md-sys-color-on-surface-variant)] text-center">
            {{ t('dialogs.backlog.prompt') }}
          </p>

          <!-- M3E Action Row -->
          <div class="flex flex-col sm:flex-row items-center justify-end gap-2 pt-1">
            <M3Button
              variant="text"
              size="s"
              class="w-full sm:w-auto"
              :disabled="loading"
              @click="emit('cancel')"
            >
              {{ t('dialogs.backlog.cancel') }}
            </M3Button>
            <M3Button
              variant="tonal"
              size="s"
              class="w-full sm:w-auto"
              :disabled="loading"
              @click="emit('confirm', false)"
            >
              {{ t('dialogs.backlog.watchOnly') }}
            </M3Button>
            <M3Button
              variant="filled"
              size="m"
              class="w-full sm:w-auto"
              :disabled="loading"
              :loading="loading"
              @click="emit('confirm', true)"
            >
              {{ t('dialogs.backlog.backupAndWatch') }}
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
