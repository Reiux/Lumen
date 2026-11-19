<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import M3Switch from './ui/M3Switch.vue'
import M3LoadingIndicator from './ui/M3LoadingIndicator.vue'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

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

const props = defineProps<{
  open: boolean
  summary: ReviewSummary | null
  includeSubfolders?: boolean
  isRescanning?: boolean
}>()

const emit = defineEmits<{
  (e: 'cancel'): void
  (e: 'confirm'): void
  (e: 'update:includeSubfolders', value: boolean): void
}>()

const dialogCardRef = ref<HTMLElement | null>(null)
useDialogFocusTrap(() => Boolean(props.open && props.summary), dialogCardRef, {
  onEscape: () => emit('cancel'),
})

function formatBytes(bytes: number, decimals = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(sizes.length - 1, Math.floor(Math.log(bytes) / Math.log(k)))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(decimals))} ${sizes[i]}`
}
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="open && summary"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
        @click.self="emit('cancel')"
      >
        <div
          ref="dialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex flex-col gap-5 w-full max-w-lg rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl max-h-[85vh] outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
          role="dialog"
          aria-modal="true"
          aria-labelledby="review-confirm-title"
          aria-describedby="review-confirm-desc"
        >
          <!-- M3E Header -->
          <div class="flex items-center gap-3.5">
            <div class="flex size-12 shrink-0 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
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
          <p
            id="review-confirm-desc"
            class="m3-body-medium leading-relaxed text-[var(--md-sys-color-on-surface-variant)]"
          >
            {{ t('dialogs.review.description') }}
          </p>

          <!-- Include Subfolders Toggle (tonal surface-container, no border hack) -->
          <div
            v-if="summary.hasFolder"
            class="flex items-center justify-between gap-4 rounded-[20px] bg-[var(--md-sys-color-surface-container)] px-4 py-3.5"
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

          <!-- M3E Grouped Segmented File List (4px gaps, 20px outer / 4px inner corners, no divide-y borders) -->
          <div class="flex flex-col gap-1 max-h-64 overflow-y-auto pr-0.5">
            <div
              v-for="(item, idx) in summary.items"
              :key="idx"
              class="flex items-center justify-between px-4 py-3 gap-3 bg-[var(--md-sys-color-surface-container)] hover:bg-[var(--md-sys-color-surface-container-highest)] transition-colors"
              :class="[
                summary.items.length === 1
                  ? 'rounded-[20px]'
                  : idx === 0
                    ? 'rounded-t-[20px] rounded-b-[4px]'
                    : idx === summary.items.length - 1
                      ? 'rounded-t-[4px] rounded-b-[20px]'
                      : 'rounded-[4px]',
              ]"
            >
              <div class="flex flex-col min-w-0 pr-2">
                <span class="m3-label-large text-[var(--md-sys-color-on-surface)] truncate font-mono">
                  {{ item.name || item.path.split(/[/\\]/).pop() || t('upload.unknownFile') }}
                </span>
                <span class="m3-body-small text-[var(--md-sys-color-on-surface-variant)] truncate">
                  {{ item.path }}
                </span>
              </div>
              <span class="rounded-full bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)] px-2.5 py-0.5 m3-label-large text-xs tabular-nums shrink-0">
                {{ formatBytes(item.sizeBytes) }}
              </span>
            </div>
          </div>

          <!-- M3E Actions: Text Cancel + Primary Filled Pill -->
          <div class="flex flex-col sm:flex-row items-center justify-end gap-2 pt-1">
            <M3Button
              variant="text"
              size="s"
              class="w-full sm:w-auto"
              @click="emit('cancel')"
            >
              {{ t('dialogs.review.cancel') }}
            </M3Button>
            <M3Button
              variant="filled"
              size="m"
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
