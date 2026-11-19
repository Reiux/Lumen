<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import M3TextField from './ui/M3TextField.vue'
import { displayNameFromEmail, getCustomDisplayName, saveCustomDisplayName } from '../utils/avatar'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

const { t } = useI18n()

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
const dialogCardRef = ref<HTMLElement | null>(null)
useDialogFocusTrap(() => props.modelValue, dialogCardRef, { onEscape: close })

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
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="close"
      >
        <div
          ref="dialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex w-full max-w-[440px] flex-col gap-5 rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
          role="dialog"
          aria-modal="true"
          aria-labelledby="edit-display-name-title"
          aria-describedby="edit-display-name-desc"
        >
          <!-- M3E Centered Hero Icon + Headline -->
          <div class="flex flex-col items-center text-center gap-3">
            <div class="flex size-14 items-center justify-center rounded-[20px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
              <M3Icon
                name="badge"
                :filled="true"
                :size="28"
              />
            </div>
            <div class="flex flex-col gap-1">
              <h2
                id="edit-display-name-title"
                class="m3-headline-small text-[var(--md-sys-color-on-surface)] select-none"
              >
                {{ t('dialogs.editName.title') }}
              </h2>
              <p
                id="edit-display-name-desc"
                class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)] select-none"
              >
                {{ t('dialogs.editName.forAccount', { email: accountEmail }) }}
              </p>
            </div>
          </div>

          <div class="w-full text-left flex flex-col gap-1.5">
            <M3TextField
              id="edit-display-name-input"
              v-model="localDisplayName"
              :label="t('dialogs.editName.label')"
              :placeholder="t('dialogs.editName.placeholder')"
              variant="outlined"
              leading-icon="person"
              autofocus
              @keydown.enter="handleSave"
            />
            <span class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)] px-1">
              {{ t('dialogs.editName.subtitle') }}
            </span>
          </div>

          <!-- M3E Action Row: >= 40px secondary targets + 48px primary pill -->
          <div class="flex w-full items-center justify-between gap-2 pt-1">
            <M3Button
              variant="text"
              size="s"
              destructive
              @click="handleReset"
            >
              {{ t('dialogs.editName.reset') }}
            </M3Button>
            <div class="flex items-center gap-2">
              <M3Button
                variant="text"
                size="s"
                @click="close"
              >
                {{ t('dialogs.editName.cancel') }}
              </M3Button>
              <M3Button
                variant="filled"
                size="m"
                @click="handleSave"
              >
                {{ t('dialogs.editName.save') }}
              </M3Button>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
