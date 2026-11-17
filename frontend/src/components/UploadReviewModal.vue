<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import M3Switch from './ui/M3Switch.vue'
import M3LoadingIndicator from './ui/M3LoadingIndicator.vue'

const { t } = useI18n()

export interface ReviewItem {
  name: string
  path: string
  sizeBytes: number
}

export interface ReviewSummary {
  items: ReviewItem[]
  totalCount: number
  totalBytes: number
  hasFolder?: boolean
}

defineProps<{
  summary: ReviewSummary
  includeSubfolders?: boolean
  isRescanning?: boolean
}>()

const emit = defineEmits<{
  (e: 'cancel'): void
  (e: 'confirm'): void
  (e: 'update:includeSubfolders', value: boolean): void
}>()

function formatBytes(bytes: number, decimals = 1): string {
  if (!bytes || bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(decimals))} ${sizes[i]}`
}
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
        @click.self="emit('cancel')"
      >
        <div
          class="m3-dialog-card relative flex flex-col gap-4 w-full max-w-lg rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl border border-[var(--md-sys-color-outline-variant)]/40 max-h-[85vh]"
          role="dialog"
          aria-modal="true"
          aria-labelledby="review-confirm-title"
        >
          <!-- Header -->
          <div class="flex items-center gap-3 text-[var(--md-sys-color-primary)]">
            <div class="flex size-11 items-center justify-center rounded-[18px] bg-[var(--md-sys-color-primary-container)]">
              <M3Icon
                name="cloud_upload"
                :filled="true"
                :size="24"
              />
            </div>
            <div class="flex flex-col min-w-0">
              <h3
                id="review-confirm-title"
                class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
              >
                {{ t('dialogs.review.title') }}
              </h3>
              <span class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)] mt-0.5">
                {{ t('dialogs.review.found', { count: summary.totalCount, bytes: formatBytes(summary.totalBytes) }) }}
              </span>
            </div>
          </div>

          <!-- Description -->
          <p class="m3-body-medium">
            {{ t('dialogs.review.description') }}
          </p>

          <!-- Include Subfolders Toggle (folder selections only, rescans on change) -->
          <div
            v-if="summary.hasFolder"
            class="flex items-center justify-between gap-4 rounded-2xl border border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-lowest)] px-4 py-3"
          >
            <div class="flex flex-col pr-2">
              <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                {{ t('dialogs.review.includeSubfolders') }}
              </span>
              <span class="m3-body-medium mt-0.5 text-[var(--md-sys-color-on-surface-variant)]">
                {{ t('dialogs.review.includeSubfoldersSubtitle') }}
              </span>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <M3LoadingIndicator
                v-if="isRescanning"
                size="xs"
                class="text-[var(--md-sys-color-primary)]"
              />
              <M3Switch
                :model-value="includeSubfolders"
                :disabled="isRescanning"
                :aria-label="t('dialogs.review.includeSubfolders')"
                @update:model-value="emit('update:includeSubfolders', $event)"
              />
            </div>
          </div>

          <!-- Scrollable files list -->
          <div class="flex flex-col max-h-64 overflow-y-auto rounded-2xl bg-[var(--md-sys-color-surface-container)] border border-[var(--md-sys-color-outline-variant)]/30 divide-y divide-[var(--md-sys-color-outline-variant)]/20">
            <div
              v-for="(item, idx) in summary.items"
              :key="idx"
              class="flex items-center justify-between p-3 gap-3 hover:bg-[var(--md-sys-color-surface-container-highest)]/40 transition-colors"
            >
              <div class="flex flex-col min-w-0 pr-2">
                <span class="m3-label-large text-[var(--md-sys-color-on-surface)] truncate font-mono">
                  {{ item.name || item.path.split(/[/\\]/).pop() || t('upload.unknownFile') }}
                </span>
                <span class="m3-body-small text-[var(--md-sys-color-outline)] truncate">
                  {{ item.path }}
                </span>
              </div>
              <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)] tabular-nums shrink-0">
                {{ formatBytes(item.sizeBytes) }}
              </span>
            </div>
          </div>

          <!-- Actions: Cancel and Start Backup -->
          <div class="flex flex-col sm:flex-row items-center justify-end gap-2 pt-2">
            <M3Button
              variant="outlined"
              size="s"
              class="w-full sm:w-auto"
              @click="emit('cancel')"
            >
              {{ t('dialogs.review.cancel') }}
            </M3Button>
            <M3Button
              variant="filled"
              size="s"
              icon="cloud_upload"
              class="w-full sm:w-auto"
              @click="emit('confirm')"
            >
              {{ t('dialogs.review.startBackup') }} ({{ summary.totalCount }})
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
