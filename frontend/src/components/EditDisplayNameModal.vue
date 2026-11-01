<script setup lang="ts">
import { ref, watch } from 'vue'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import M3TextField from './ui/M3TextField.vue'
import { displayNameFromEmail, getCustomDisplayName, saveCustomDisplayName } from '../utils/avatar'

const props = defineProps<{
  modelValue: boolean
  accountEmail: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'saved', newName: string): void
  (e: 'reset'): void
}>()

const localDisplayName = ref('')

watch(
  () => [props.modelValue, props.accountEmail],
  ([isOpen, email]) => {
    if (isOpen && typeof email === 'string' && email) {
      localDisplayName.value = getCustomDisplayName(email) || displayNameFromEmail(email)
    }
  },
  { immediate: true },
)

function close() {
  emit('update:modelValue', false)
}

function handleSave() {
  if (!props.accountEmail) return
  const trimmed = localDisplayName.value.trim()
  saveCustomDisplayName(props.accountEmail, trimmed)
  emit('saved', trimmed)
  close()
}

function handleReset() {
  if (!props.accountEmail) return
  saveCustomDisplayName(props.accountEmail, '')
  emit('reset')
  close()
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
          aria-labelledby="edit-display-name-title"
        >
          <div class="flex size-16 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
            <M3Icon
              name="badge"
              :filled="true"
              :size="32"
            />
          </div>
          <div class="flex flex-col gap-1">
            <h2
              id="edit-display-name-title"
              class="m3-headline-small text-[var(--md-sys-color-on-surface)] select-none"
            >
              Change Display Name
            </h2>
            <p class="m3-body-medium select-none">
              For account <strong class="text-[var(--md-sys-color-on-surface)]">{{ accountEmail }}</strong>
            </p>
          </div>

          <div class="w-full text-left flex flex-col gap-1.5">
            <M3TextField
              id="edit-display-name-input"
              v-model="localDisplayName"
              label="Display Name"
              placeholder="e.g. Alex"
              variant="outlined"
              leading-icon="person"
              autofocus
              @keydown.enter="handleSave"
            />
            <span class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)] px-1">
              Saved locally on this device since login credentials only include an OAuth token.
            </span>
          </div>

          <div class="flex w-full items-center justify-between gap-3 mt-1">
            <M3Button
              variant="text"
              size="xs"
              destructive
              @click="handleReset"
            >
              Reset to default
            </M3Button>
            <div class="flex items-center gap-2">
              <M3Button
                variant="text"
                size="s"
                @click="close"
              >
                Cancel
              </M3Button>
              <M3Button
                variant="filled"
                size="s"
                @click="handleSave"
              >
                Save
              </M3Button>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
