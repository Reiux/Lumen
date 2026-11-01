<script setup lang="ts">
import { computed, ref } from 'vue'
import { Browser } from '@wailsio/runtime'
import { toast } from 'vue-sonner'
import { ConfigManager } from '../../bindings/app/core'
import { uploadManager } from '../utils/UploadManager'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import RemoveAccountModal from './RemoveAccountModal.vue'

const emit = defineEmits<{
  (event: 'open-sign-in'): void
  (event: 'account-removed'): void
}>()

const blockedState = computed(() => uploadManager.state.accountBlocked)
const isVisible = computed(() => blockedState.value.isBlocked)
const showSafetyDialog = ref(false)
const showRemoveModal = ref(false)
const isRemoving = ref(false)

const resumeTimeFormatted = computed(() => {
  if (!blockedState.value.resumeAt) return '15 minutes'
  try {
    const d = new Date(blockedState.value.resumeAt)
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
  } catch {
    return '15 minutes'
  }
})

function dismiss() {
  uploadManager.clearAccountBlocked()
}

function onSignInAgain() {
  uploadManager.clearAccountBlocked()
  emit('open-sign-in')
}

async function openAccountDetails() {
  try {
    await Browser.OpenURL('https://myaccount.google.com/security')
  } catch (error) {
    toast.error('Could not open browser', {
      description: error instanceof Error ? error.message : String(error),
    })
  }
}

async function removeAccount() {
  const email = blockedState.value.email
  if (!email) {
    uploadManager.clearAccountBlocked()
    return
  }
  isRemoving.value = true
  try {
    await ConfigManager.RemoveCredentials(email)
    uploadManager.clearAccountBlocked()
    emit('account-removed')
    toast.success('Account removed', {
      description: email,
    })
    showRemoveModal.value = false
  } catch (error) {
    toast.error('Failed to remove account', {
      description: error instanceof Error ? error.message : String(error),
    })
  } finally {
    isRemoving.value = false
  }
}
</script>

<template>
  <Transition name="fade">
    <div
      v-if="isVisible"
      class="m3-circuit-alert relative z-30 mb-6 flex flex-col gap-4 rounded-[28px] border p-5 shadow-sm select-none backdrop-blur-md transition-all"
      :class="[
        blockedState.reason === 'ACCOUNT_RESTRICTED'
          ? 'border-[var(--md-sys-color-error)]/40 bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)]'
          : 'border-[var(--md-sys-color-tertiary)]/40 bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)]'
      ]"
    >
      <div class="flex items-start gap-3.5">
        <!-- Icon (28px inside a 44x44 rounded-[16px] container) -->
        <div
          class="flex size-11 shrink-0 items-center justify-center rounded-[16px] bg-black/10 dark:bg-white/10"
        >
          <M3Icon
            :name="blockedState.reason === 'ACCOUNT_RESTRICTED' ? 'gpp_bad' : blockedState.reason === 'RATE_LIMITED' ? 'hourglass_top' : 'lock_reset'"
            :size="26"
            :filled="true"
          />
        </div>

        <!-- Content -->
        <div class="flex flex-1 flex-col gap-1.5 min-w-0">
          <div class="flex items-center justify-between gap-2">
            <h4 class="m3-title-medium">
              <template v-if="blockedState.reason === 'SIGN_IN_REQUIRED'">
                Sign in required
              </template>
              <template v-else-if="blockedState.reason === 'ACCOUNT_RESTRICTED'">
                Account restricted
              </template>
              <template v-else>
                Uploads paused
              </template>
            </h4>
            <button
              type="button"
              class="flex size-7 items-center justify-center rounded-full opacity-70 hover:opacity-100 hover:bg-black/10 dark:hover:bg-white/10 transition cursor-pointer"
              aria-label="Dismiss alert"
              @click="dismiss"
            >
              <M3Icon name="close" :size="16" />
            </button>
          </div>

          <p class="m3-body-medium leading-relaxed opacity-95">
            <template v-if="blockedState.reason === 'SIGN_IN_REQUIRED'">
              Google is asking you to sign in again. Uploads are paused to protect your account from security lockouts.
            </template>
            <template v-else-if="blockedState.reason === 'ACCOUNT_RESTRICTED'">
              Google has restricted this account. Uploads are stopped to prevent further action on your account. Check your Google account directly; do not keep retrying.
            </template>
            <template v-else>
              Google asked Lumen to slow down. Automatic backoff is active; uploads will resume automatically at {{ resumeTimeFormatted }}.
            </template>
          </p>

          <!-- Action buttons with M3 Expressive styling -->
          <div class="mt-2 flex flex-wrap items-center gap-2.5">
            <M3Button
              variant="tonal"
              size="xs"
              icon="shield"
              @click="showSafetyDialog = true"
            >
              <span>Account safety</span>
            </M3Button>

            <template v-if="blockedState.reason === 'SIGN_IN_REQUIRED'">
              <M3Button
                variant="filled"
                size="xs"
                icon="login"
                @click="onSignInAgain"
              >
                <span>Sign in again</span>
              </M3Button>
            </template>

            <template v-else-if="blockedState.reason === 'ACCOUNT_RESTRICTED'">
              <M3Button
                variant="tonal"
                size="xs"
                icon="open_in_new"
                @click="openAccountDetails"
              >
                <span>Account details</span>
              </M3Button>
              <M3Button
                variant="filled"
                size="xs"
                icon="delete"
                :destructive="true"
                @click="showRemoveModal = true"
              >
                <span>Remove account</span>
              </M3Button>
            </template>

            <template v-else>
              <M3Button
                variant="filled"
                size="xs"
                icon="check"
                @click="dismiss"
              >
                <span>Dismiss</span>
              </M3Button>
            </template>
          </div>
        </div>
      </div>
    </div>
  </Transition>

  <!-- M3 Expressive Account Safety Info Modal -->
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="showSafetyDialog"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
        @click.self="showSafetyDialog = false"
      >
        <div
          class="m3-dialog-card relative flex flex-col w-full max-w-md gap-4 rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl"
          role="dialog"
          aria-modal="true"
          aria-labelledby="safety-notice-title"
        >
          <div class="flex items-center gap-3 text-[var(--md-sys-color-primary)]">
            <div class="flex size-10 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-primary-container)]">
              <M3Icon name="shield" :size="22" :filled="true" />
            </div>
            <h3
              id="safety-notice-title"
              class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
            >
              Account Safety Notice
            </h3>
          </div>

          <div class="flex flex-col gap-2.5 m3-body-medium leading-relaxed">
            <p>
              Lumen is an unofficial client for Google Photos that uses authenticated session tokens.
            </p>
            <p>
              To protect your Google account from automated abuse restrictions, Lumen includes a built-in safety circuit breaker that detects rate limits and authorization changes.
            </p>
            <ul class="list-disc pl-4 space-y-1">
              <li>Uploads pause immediately when rate limits are approached.</li>
              <li>Repeated failures back off automatically to avoid account flagging.</li>
              <li>Always keep your original photo files saved safely.</li>
            </ul>
          </div>

          <div class="flex justify-end pt-2">
            <M3Button
              variant="filled"
              size="s"
              icon="check"
              @click="showSafetyDialog = false"
            >
              <span>Got it</span>
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>

  <!-- Remove Account Confirmation Dialog -->
  <RemoveAccountModal
    v-model="showRemoveModal"
    :account-email="blockedState.email"
    :loading="isRemoving"
    @confirm="removeAccount"
  />
</template>
