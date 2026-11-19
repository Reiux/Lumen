<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import { displayNameFromEmail, getCustomDisplayName } from '../utils/avatar'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

const { t } = useI18n()

const props = defineProps<{
  modelValue: boolean
  accountEmail: string
  loading?: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'confirm', email: string): void
}>()

const dialogCardRef = ref<HTMLElement | null>(null)
useDialogFocusTrap(() => props.modelValue, dialogCardRef, { onEscape: close })

const resolvedDisplayName = computed(() => {
  if (!props.accountEmail) return ''
  return getCustomDisplayName(props.accountEmail) || displayNameFromEmail(props.accountEmail)
})

function close() {
  if (props.loading) return
  emit('update:modelValue', false)
}

function handleConfirm() {
  emit('confirm', props.accountEmail)
}
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="modelValue"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="close"
      >
        <div
          ref="dialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex w-full max-w-[440px] flex-col gap-5 rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="remove-account-title"
          aria-describedby="remove-account-desc"
        >
          <!-- M3E Centered Hero Icon + Headline -->
          <div class="flex flex-col items-center text-center gap-3">
            <div class="flex size-14 items-center justify-center rounded-[20px] bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]">
              <M3Icon
                name="person_remove"
                :filled="true"
                :size="28"
              />
            </div>

            <div class="flex flex-col gap-1.5">
              <h2
                id="remove-account-title"
                class="m3-headline-small text-[var(--md-sys-color-on-surface)] select-none"
              >
                {{ t('dialogs.removeAccount.title') }}
              </h2>
              <p
                id="remove-account-desc"
                class="m3-body-medium leading-relaxed text-[var(--md-sys-color-on-surface-variant)] select-none"
              >
                {{ t('dialogs.removeAccount.confirmPrompt', { name: resolvedDisplayName || accountEmail }) }}
              </p>
            </div>
          </div>

          <!-- Tonal Error Warning Container (no arbitrary opacity/border hacks) -->
          <div class="flex items-start gap-3 rounded-[16px] bg-[var(--md-sys-color-error-container)] px-4 py-3.5 text-[var(--md-sys-color-on-error-container)]">
            <M3Icon
              name="warning"
              :filled="true"
              :size="20"
              class="shrink-0 mt-0.5"
            />
            <p class="m3-body-medium leading-snug !text-[var(--md-sys-color-on-error-container)] select-none">
              {{ t('dialogs.removeAccount.warning') }}
            </p>
          </div>

          <!-- M3E Action Row: secondary text action + larger primary destructive pill -->
          <div class="flex w-full items-center justify-end gap-2 pt-1">
            <M3Button
              variant="text"
              size="s"
              :disabled="loading"
              @click="close"
            >
              <span>{{ t('dialogs.removeAccount.cancel') }}</span>
            </M3Button>
            <M3Button
              variant="filled"
              destructive
              size="m"
              icon="delete"
              :loading="loading"
              @click="handleConfirm"
            >
              <span>{{ t('dialogs.removeAccount.remove') }}</span>
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
