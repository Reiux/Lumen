<script setup lang="ts">
import { computed } from 'vue'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import { displayNameFromEmail, getCustomDisplayName } from '../utils/avatar'

const props = defineProps<{
  modelValue: boolean
  accountEmail: string
  loading?: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'confirm', email: string): void
}>()

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
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="close"
      >
        <div
          class="m3-dialog-card relative flex w-full max-w-md flex-col items-center justify-center gap-5 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-high)] p-8 text-center shadow-2xl"
          role="dialog"
          aria-modal="true"
          aria-labelledby="remove-account-title"
        >
          <div class="flex size-16 items-center justify-center rounded-full bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]">
            <M3Icon
              name="person_remove"
              :filled="true"
              :size="32"
            />
          </div>

          <div class="flex flex-col gap-2 text-center">
            <h2
              id="remove-account-title"
              class="m3-headline-small text-[var(--md-sys-color-on-surface)] select-none"
            >
              Remove Account
            </h2>
            <p class="m3-body-medium select-none">
              Are you sure you want to remove <strong class="text-[var(--md-sys-color-on-surface)]">{{ resolvedDisplayName || accountEmail }}</strong> from Lumen?
            </p>
            <p class="m3-body-medium text-[var(--md-sys-color-error)] select-none mt-1 leading-relaxed bg-[var(--md-sys-color-error-container)]/30 rounded-xl p-3 border border-[var(--md-sys-color-error)]/20 text-left">
              Removing and re-adding an account signs in again, which registers another device on your Google account. Only do this if needed.
            </p>
          </div>

          <div class="flex w-full items-center justify-end gap-3 pt-2">
            <M3Button
              variant="text"
              size="s"
              :disabled="loading"
              @click="close"
            >
              <span>Cancel</span>
            </M3Button>
            <M3Button
              variant="filled"
              destructive
              size="s"
              icon="delete"
              :loading="loading"
              @click="handleConfirm"
            >
              <span>Remove</span>
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
