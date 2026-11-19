<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Browser } from '@wailsio/runtime'
import { toast } from 'vue-sonner'
import { ConfigManager } from '../../bindings/app/core'
import { uploadManager } from '../utils/UploadManager'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import RemoveAccountModal from './RemoveAccountModal.vue'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

const { t } = useI18n()

const emit = defineEmits<{
  (event: 'open-sign-in'): void
  (event: 'account-removed'): void
}>()

const blockedState = computed(() => uploadManager.state.accountBlocked)
const isVisible = computed(() => blockedState.value.isBlocked)
const showSafetyDialog = ref(false)
const safetyDialogCardRef = ref<HTMLElement | null>(null)
const showRemoveModal = ref(false)
const isRemoving = ref(false)

const nowMs = ref(Date.now())
let countdownTimer: ReturnType<typeof setInterval> | null = null

function stopCountdown() {
  if (countdownTimer !== null) {
    clearInterval(countdownTimer)
    countdownTimer = null
  }
}

function tickCountdown() {
  nowMs.value = Date.now()
  const blocked = blockedState.value
  if (blocked.isBlocked && blocked.reason === 'RATE_LIMITED' && blocked.resumeAt && nowMs.value >= blocked.resumeAt) {
    stopCountdown()
    uploadManager.clearAccountBlocked()
  }
}

watch(
  () => [blockedState.value.isBlocked, blockedState.value.reason, blockedState.value.resumeAt] as const,
  ([blocked, reason, resumeAt]) => {
    stopCountdown()
    if (blocked && reason === 'RATE_LIMITED' && resumeAt) {
      nowMs.value = Date.now()
      if (nowMs.value >= resumeAt) {
        uploadManager.clearAccountBlocked()
        return
      }
      countdownTimer = setInterval(tickCountdown, 250)
    }
  },
  { immediate: true },
)

onUnmounted(() => {
  stopCountdown()
})

useDialogFocusTrap(() => showSafetyDialog.value, safetyDialogCardRef, {
  onEscape: () => {
    showSafetyDialog.value = false
  },
})

const remainingSeconds = computed(() => {
  if (!blockedState.value.resumeAt) return 30
  return Math.max(0, Math.ceil((blockedState.value.resumeAt - nowMs.value) / 1000))
})

const countdownFormatted = computed(() => {
  const total = remainingSeconds.value
  if (total >= 60) {
    const mins = Math.floor(total / 60)
    const secs = total % 60
    return `${mins}m ${String(secs).padStart(2, '0')}s`
  }
  return `${total}s`
})

function onSignInAgain() {
  emit('open-sign-in')
}

async function openAccountDetails() {
  try {
    await Browser.OpenURL('https://myaccount.google.com/security')
  } catch (error) {
    toast.error(t('circuitBreaker.toastBrowserError'), {
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
    toast.success(t('circuitBreaker.toastAccountRemoved'), {
      description: email,
    })
    showRemoveModal.value = false
  } catch (error) {
    toast.error(t('circuitBreaker.toastRemoveFailed'), {
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
                {{ t('circuitBreaker.signInRequired') }}
              </template>
              <template v-else-if="blockedState.reason === 'ACCOUNT_RESTRICTED'">
                {{ t('circuitBreaker.accountRestricted') }}
              </template>
              <template v-else>
                {{ t('circuitBreaker.rateLimited') }}
              </template>
            </h4>
            <span
              v-if="blockedState.reason === 'RATE_LIMITED'"
              class="inline-flex items-center gap-1 rounded-full bg-black/10 dark:bg-white/10 px-2.5 py-0.5 m3-label-medium tabular-nums shrink-0"
            >
              <M3Icon
                name="timer"
                :size="14"
              />
              <span>{{ countdownFormatted }}</span>
            </span>
          </div>

          <p
            class="m3-body-medium leading-relaxed"
            :class="blockedState.reason === 'ACCOUNT_RESTRICTED'
              ? '!text-[var(--md-sys-color-on-error-container)]'
              : '!text-[var(--md-sys-color-on-tertiary-container)]'"
          >
            <template v-if="blockedState.reason === 'SIGN_IN_REQUIRED'">
              {{ t('circuitBreaker.signInRequiredDesc') }}
            </template>
            <template v-else-if="blockedState.reason === 'ACCOUNT_RESTRICTED'">
              {{ t('circuitBreaker.accountRestrictedDesc') }}
            </template>
            <template v-else>
              {{ t('circuitBreaker.rateLimitedDesc', { time: countdownFormatted }) }}
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
              <span>{{ t('circuitBreaker.accountSafety') }}</span>
            </M3Button>

            <template v-if="blockedState.reason === 'SIGN_IN_REQUIRED'">
              <M3Button
                variant="filled"
                size="xs"
                icon="login"
                @click="onSignInAgain"
              >
                <span>{{ t('circuitBreaker.signInAgain') }}</span>
              </M3Button>
            </template>

            <template v-else-if="blockedState.reason === 'ACCOUNT_RESTRICTED'">
              <M3Button
                variant="tonal"
                size="xs"
                icon="open_in_new"
                @click="openAccountDetails"
              >
                <span>{{ t('circuitBreaker.accountDetails') }}</span>
              </M3Button>
              <M3Button
                variant="filled"
                size="xs"
                icon="delete"
                :destructive="true"
                @click="showRemoveModal = true"
              >
                <span>{{ t('circuitBreaker.removeAccount') }}</span>
              </M3Button>
            </template>
          </div>
        </div>
      </div>
    </div>
  </Transition>

  <!-- M3 Expressive Account Safety Dialog (28dp surface-container-high, hero icon, grouped segmented list) -->
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="showSafetyDialog"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/65 backdrop-blur-xs p-4 select-none overflow-y-auto"
        @click.self="showSafetyDialog = false"
      >
        <div
          ref="safetyDialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex flex-col w-full max-w-[440px] gap-5 rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] p-6 shadow-2xl outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)]"
          role="dialog"
          aria-modal="true"
          aria-labelledby="safety-notice-title"
          aria-describedby="safety-notice-desc"
        >
          <!-- M3E Centered Hero Icon + Headline -->
          <div class="flex flex-col items-center text-center gap-3">
            <div class="flex size-14 items-center justify-center rounded-[20px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
              <M3Icon
                name="shield"
                :size="28"
                :filled="true"
              />
            </div>
            <div class="flex flex-col gap-1.5">
              <h3
                id="safety-notice-title"
                class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
              >
                {{ t('circuitBreaker.safetyModalTitle') }}
              </h3>
              <p
                id="safety-notice-desc"
                class="m3-body-medium leading-relaxed text-[var(--md-sys-color-on-surface-variant)]"
              >
                {{ t('circuitBreaker.safetyModalP1') }}
              </p>
            </div>
          </div>

          <!-- M3E Grouped Segmented List (4px gaps, 20px outer / 4px inner corners, surface-container tones) -->
          <div class="flex flex-col gap-2">
            <span class="m3-label-large text-[var(--md-sys-color-on-surface-variant)] px-1">
              {{ t('circuitBreaker.safetyModalP2') }}
            </span>

            <ul class="flex flex-col gap-1">
              <li class="flex items-start gap-3 rounded-t-[20px] rounded-b-[4px] bg-[var(--md-sys-color-surface-container)] px-4 py-3.5">
                <div class="flex size-9 shrink-0 items-center justify-center rounded-[12px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                  <M3Icon
                    name="verified_user"
                    :size="18"
                    :filled="true"
                  />
                </div>
                <span class="m3-body-medium leading-snug text-[var(--md-sys-color-on-surface)] pt-0.5">
                  {{ t('circuitBreaker.safetyModalL1') }}
                </span>
              </li>

              <li class="flex items-start gap-3 rounded-[4px] bg-[var(--md-sys-color-surface-container)] px-4 py-3.5">
                <div class="flex size-9 shrink-0 items-center justify-center rounded-[12px] bg-[var(--md-sys-color-secondary-container)] text-[var(--md-sys-color-on-secondary-container)]">
                  <M3Icon
                    name="speed"
                    :size="18"
                    :filled="true"
                  />
                </div>
                <span class="m3-body-medium leading-snug text-[var(--md-sys-color-on-surface)] pt-0.5">
                  {{ t('circuitBreaker.safetyModalL2') }}
                </span>
              </li>

              <li class="flex items-start gap-3 rounded-t-[4px] rounded-b-[20px] bg-[var(--md-sys-color-surface-container)] px-4 py-3.5">
                <div class="flex size-9 shrink-0 items-center justify-center rounded-[12px] bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)]">
                  <M3Icon
                    name="pause_circle"
                    :size="18"
                    :filled="true"
                  />
                </div>
                <span class="m3-body-medium leading-snug text-[var(--md-sys-color-on-surface)] pt-0.5">
                  {{ t('circuitBreaker.safetyModalL3') }}
                </span>
              </li>
            </ul>
          </div>

          <!-- M3E Action Row (right-aligned pill button, >= 48px target) -->
          <div class="flex justify-end pt-1">
            <M3Button
              variant="filled"
              size="m"
              icon="check"
              class="min-h-12 px-6"
              @click="showSafetyDialog = false"
            >
              <span>{{ t('circuitBreaker.gotIt') }}</span>
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
