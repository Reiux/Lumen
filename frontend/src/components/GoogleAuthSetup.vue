<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Browser, Clipboard, Events } from '@wailsio/runtime'
import { toast } from 'vue-sonner'
import { ConfigManager } from '../../bindings/app/core'
import M3Button from './ui/M3Button.vue'
import M3ButtonGroup from './ui/M3ButtonGroup.vue'
import M3Icon from './ui/M3Icon.vue'
import M3LoadingIndicator from './ui/M3LoadingIndicator.vue'
import M3TextField from './ui/M3TextField.vue'
import AccountRiskCard from './AccountRiskCard.vue'
import { displayNameFromEmail, getAvatarColor, getEmailInitial } from '../utils/avatar'
import { useDialogFocusTrap } from '../utils/useDialogFocusTrap'

const { t } = useI18n()

const embeddedSetupURL = 'https://accounts.google.com/EmbeddedSetup'

const emit = defineEmits<{
  (event: 'account-added'): void
}>()

const isOpen = defineModel<boolean>('open', { default: false })

// Workflow state
const authMode = ref<'oauth' | 'direct'>('oauth')
const oauthToken = ref('')
const rawCredential = ref('')
const hasAcknowledgedRisk = ref(false)
const isValidating = ref(false)
const validationError = ref('')
const verifiedUser = ref<{ email: string } | null>(null)

// Sign-in state
const browserState = ref<'idle' | 'waiting' | 'waiting-external' | 'error'>('idle')
const browserErrorMessage = ref('')
const browserCloseFallbackMessage = ref('')
const hasExternalChromium = ref(false)
const detectedBrowserName = ref('')
const showManualFallback = ref(false)
let unlistenCloseFailed: (() => void) | null = null

const dialogCardRef = ref<HTMLElement | null>(null)
const dialogBodyRef = ref<HTMLElement | null>(null)
const tokenFieldRef = ref<{ focus?: () => void } | null>(null)
let successDismissTimer: ReturnType<typeof setTimeout> | null = null
let browserAttemptSeq = 0
let connectAttemptSeq = 0

function clearSuccessDismissTimer() {
  if (successDismissTimer) {
    clearTimeout(successDismissTimer)
    successDismissTimer = null
  }
}

function closeDialog() {
  if (isValidating.value) return
  isOpen.value = false
}

useDialogFocusTrap(
  () => isOpen.value,
  dialogCardRef,
  {
    onEscape: closeDialog,
    initialFocus: () => tokenFieldRef.value,
  },
)

async function checkBrowserSupport() {
  try {
    const hasChromium = await ConfigManager.HasSupportedBrowser().catch(() => false)
    const browserName = hasChromium ? (await ConfigManager.GetSupportedBrowserName().catch(() => '')) || 'Chrome' : ''
    if (!isOpen.value) return
    hasExternalChromium.value = !!hasChromium
    detectedBrowserName.value = browserName
  } catch {
    if (!isOpen.value) return
    hasExternalChromium.value = false
    detectedBrowserName.value = ''
  }
}

watch(hasAcknowledgedRisk, (ack) => {
  ConfigManager.SetAccountRiskAcknowledged(ack).catch(() => {})
})

function handleSignInSuccess(connectedEmail: string) {
  browserState.value = 'idle'
  verifiedUser.value = { email: connectedEmail }

  clearSuccessDismissTimer()
  successDismissTimer = setTimeout(() => {
    successDismissTimer = null
    if (!verifiedUser.value) return
    const email = verifiedUser.value.email
    verifiedUser.value = null
    isOpen.value = false
    emit('account-added')
    toast.success(t('googleAuth.toastConnected'), {
      description: email,
    })
  }, 1500)
}

async function handleEmbeddedSignIn() {
  if (!hasAcknowledgedRisk.value) {
    validationError.value = t('googleAuth.validationSafetyNotice')
    return
  }

  const attemptSeq = ++browserAttemptSeq
  browserState.value = 'waiting'
  browserErrorMessage.value = ''
  browserCloseFallbackMessage.value = ''
  validationError.value = ''
  showManualFallback.value = false

  try {
    await ConfigManager.SetAccountRiskAcknowledged(true)
    if (!isOpen.value || attemptSeq !== browserAttemptSeq) return

    const connectedEmail = await ConfigManager.StartEmbeddedSignIn()
    if (attemptSeq !== browserAttemptSeq) return
    if (!isOpen.value) {
      if (connectedEmail) {
        emit('account-added')
      }
      return
    }
    if (!connectedEmail) {
      throw new Error(t('googleAuth.couldNotVerifyNoEmail'))
    }

    handleSignInSuccess(connectedEmail)
  } catch (error: unknown) {
    if (!isOpen.value || attemptSeq !== browserAttemptSeq) return
    const errMsg = error instanceof Error ? error.message : String(error)
    if (errMsg.toLowerCase().includes('cancel')) {
      browserState.value = 'idle'
      return
    }
    browserState.value = 'error'
    browserErrorMessage.value = errMsg || t('googleAuth.couldNotSignIn')
  }
}

async function handleExternalBrowserSignIn() {
  if (!hasAcknowledgedRisk.value) {
    validationError.value = t('googleAuth.validationSafetyNotice')
    return
  }

  const attemptSeq = ++browserAttemptSeq
  try {
    await ConfigManager.CancelBrowserSignIn()
  } catch {
    // ignore prior session cleanup errors
  }
  if (!isOpen.value || attemptSeq !== browserAttemptSeq) return

  browserState.value = 'waiting-external'
  browserErrorMessage.value = ''
  browserCloseFallbackMessage.value = ''
  validationError.value = ''
  showManualFallback.value = false

  try {
    await ConfigManager.SetAccountRiskAcknowledged(true)
    if (!isOpen.value || attemptSeq !== browserAttemptSeq) return
    const connectedEmail = await ConfigManager.StartExternalBrowserSignIn()
    if (attemptSeq !== browserAttemptSeq) return
    if (!isOpen.value) {
      if (connectedEmail) {
        emit('account-added')
      }
      return
    }
    if (!connectedEmail) {
      throw new Error(t('googleAuth.couldNotVerifyNoEmail'))
    }

    handleSignInSuccess(connectedEmail)
  } catch (error: unknown) {
    if (!isOpen.value || attemptSeq !== browserAttemptSeq) return
    const errMsg = error instanceof Error ? error.message : String(error)
    if (errMsg.toLowerCase().includes('cancel')) {
      browserState.value = 'idle'
      return
    }
    browserState.value = 'error'
    browserErrorMessage.value = errMsg || t('googleAuth.couldNotSignIn')
  }
}

async function handleCancelBrowserSignIn() {
  browserAttemptSeq++
  browserState.value = 'idle'
  try {
    await ConfigManager.CancelBrowserSignIn()
  } catch {
    // ignore
  }
}

async function handleFallbackToManual() {
  if (browserState.value === 'waiting' || browserState.value === 'waiting-external') {
    await handleCancelBrowserSignIn()
  }
  browserState.value = 'idle'
  showManualFallback.value = true
  nextTick(() => {
    tokenFieldRef.value?.focus?.()
  })
}

watch(isOpen, (open) => {
  if (open) {
    validationError.value = ''
    verifiedUser.value = null
    isValidating.value = false
    browserState.value = 'idle'
    browserErrorMessage.value = ''
    browserCloseFallbackMessage.value = ''
    showManualFallback.value = false
    checkBrowserSupport()
    nextTick(() => {
      if (dialogBodyRef.value) {
        dialogBodyRef.value.scrollTop = 0
      }
    })
  } else {
    connectAttemptSeq++
    if (browserState.value === 'waiting' || browserState.value === 'waiting-external') {
      browserAttemptSeq++
      ConfigManager.CancelBrowserSignIn().catch(() => {})
    }
    if (verifiedUser.value) {
      const email = verifiedUser.value.email
      verifiedUser.value = null
      emit('account-added')
      toast.success(t('googleAuth.toastConnected'), {
        description: email,
      })
    }
    oauthToken.value = ''
    rawCredential.value = ''
    validationError.value = ''
    verifiedUser.value = null
    isValidating.value = false
    browserState.value = 'idle'
    browserErrorMessage.value = ''
    browserCloseFallbackMessage.value = ''
    showManualFallback.value = false
    clearSuccessDismissTimer()
  }
})

onMounted(() => {
  unlistenCloseFailed = Events.On('browser-signin:close-failed', () => {
    browserCloseFallbackMessage.value = t('googleAuth.signedInCanClose')
  })
})

onUnmounted(() => {
  connectAttemptSeq++
  if (browserState.value === 'waiting' || browserState.value === 'waiting-external') {
    browserAttemptSeq++
    ConfigManager.CancelBrowserSignIn().catch(() => {})
  }
  clearSuccessDismissTimer()
  if (unlistenCloseFailed) {
    unlistenCloseFailed()
    unlistenCloseFailed = null
  }
})

watch([oauthToken, rawCredential], () => {
  if (validationError.value) {
    validationError.value = ''
  }
})

async function openEmbeddedSetup() {
  try {
    await Browser.OpenURL(embeddedSetupURL)
  } catch (error) {
    toast.error(t('googleAuth.couldNotOpenBrowser'), {
      description: error instanceof Error ? error.message : String(error),
    })
  }
}

// Automatic clipboard auto-detection
async function pasteFromClipboard() {
  try {
    let text = ''
    try {
      text = await Clipboard.Text()
    } catch {
      text = await navigator.clipboard.readText()
    }
    text = (text || '').trim()
    if (!text) {
      toast.info(t('googleAuth.toastClipboardEmpty'))
      return
    }

    // Smart detection: Direct credential string
    if (text.includes('Email=') && text.includes('Token=')) {
      authMode.value = 'direct'
      rawCredential.value = text
      toast.success(t('googleAuth.toastDetectedCredential'))
      return
    }

    // Smart detection: oauth_token cookie string
    if (text.includes('oauth_token=')) {
      authMode.value = 'oauth'
      const match = text.match(/oauth_token=([^;]+)/)
      oauthToken.value = (match ? match[1] : text).trim().replace(/^["']|["']$/g, '')
      toast.success(t('googleAuth.toastDetectedToken'))
      return
    }

    // If active in direct tab and looks like key=value
    if (authMode.value === 'direct') {
      rawCredential.value = text
      toast.success(t('googleAuth.toastPastedCredential'))
    } else {
      oauthToken.value = text
      toast.success(t('googleAuth.toastPastedToken'))
    }
  } catch (err) {
    console.warn('Clipboard read failed:', err)
    toast.error(t('googleAuth.toastClipboardError'))
  }
}

const isFormValid = computed(() => {
  if (!hasAcknowledgedRisk.value) return false
  if (authMode.value === 'oauth') {
    return oauthToken.value.trim().length > 0
  }
  return rawCredential.value.trim().length > 0
})

async function handleConnect() {
  if (isValidating.value || verifiedUser.value) return
  if (!showManualFallback.value || browserState.value !== 'idle') return
  if (!hasAcknowledgedRisk.value) {
    validationError.value = t('googleAuth.validationSafetyNotice')
    return
  }

  const payload = authMode.value === 'oauth' ? oauthToken.value.trim() : rawCredential.value.trim()
  if (!payload) {
    validationError.value = t('googleAuth.validationEnterToken')
    return
  }

  const attemptSeq = ++connectAttemptSeq
  isValidating.value = true
  validationError.value = ''

  try {
    // Perform immediate live validation probe & storage via backend
    const connectedEmail = await ConfigManager.ValidateAndAddCredentials(payload)
    if (attemptSeq !== connectAttemptSeq) return
    if (!isOpen.value) {
      if (connectedEmail) {
        emit('account-added')
      }
      return
    }
    if (!connectedEmail) {
      throw new Error(t('googleAuth.couldNotVerifyNoEmail'))
    }

    // Visual confirmation state
    verifiedUser.value = { email: connectedEmail }
    isValidating.value = false

    // Auto-advance after brief visual confirmation
    clearSuccessDismissTimer()
    successDismissTimer = setTimeout(() => {
      successDismissTimer = null
      if (!verifiedUser.value) return
      const email = verifiedUser.value.email
      verifiedUser.value = null
      isOpen.value = false
      emit('account-added')
      toast.success(t('googleAuth.toastConnected'), {
        description: email,
      })
    }, 1500)
  } catch (error) {
    if (!isOpen.value || attemptSeq !== connectAttemptSeq) return
    isValidating.value = false
    const errMsg = error instanceof Error ? error.message : String(error)
    validationError.value = errMsg || t('googleAuth.validationErrorGeneric')
  }
}

function handleDoneSuccess() {
  clearSuccessDismissTimer()
  const email = verifiedUser.value?.email
  verifiedUser.value = null
  isOpen.value = false
  emit('account-added')
  if (email) {
    toast.success(t('googleAuth.toastConnected'), {
      description: email,
    })
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="isOpen"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="closeDialog"
      >
        <!-- Centered M3E Modal Dialog -->
        <div
          ref="dialogCardRef"
          tabindex="-1"
          class="m3-dialog-card relative flex flex-col w-full max-w-lg rounded-[28px] bg-[var(--md-sys-color-surface-container-high)] shadow-2xl outline-none focus-visible:outline-[3px] focus-visible:outline-[var(--md-sys-color-primary)] max-h-[calc(100dvh-32px)] overflow-hidden select-none"
          role="dialog"
          aria-modal="true"
          aria-labelledby="google-auth-title"
          style="--wails-draggable: none"
        >
          <!-- Sticky M3E Header -->
          <div class="sticky top-0 z-20 flex shrink-0 items-center justify-between gap-3 bg-[var(--md-sys-color-surface-container-high)] px-6 pt-5 pb-4">
            <div class="flex items-center gap-3.5 min-w-0">
              <div class="flex size-12 shrink-0 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                <M3Icon
                  name="lock"
                  :filled="true"
                  :size="24"
                />
              </div>
              <div class="flex flex-col min-w-0">
                <h2
                  id="google-auth-title"
                  class="m3-headline-small text-[var(--md-sys-color-on-surface)]"
                >
                  {{ t('googleAuth.title') }}
                </h2>
                <p class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)]">
                  {{ t('googleAuth.automatedSignInDesc') }}
                </p>
              </div>
            </div>

            <M3Button
              variant="text"
              size="s"
              icon="close"
              :disabled="isValidating"
              :aria-label="t('googleAuth.closeAria')"
              class="!size-10 !min-w-0 !p-0 shrink-0"
              @click="closeDialog"
            />
          </div>

          <!-- Scrollable Body -->
          <div
            ref="dialogBodyRef"
            class="m3-dialog-body flex-1 overflow-y-auto px-6 py-2 flex flex-col gap-5"
          >
            <!-- Step 1: Account Safety Gate -->
            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between px-1">
                <span class="m3-label-large text-[var(--md-sys-color-on-surface-variant)]">
                  {{ t('googleAuth.step1Title') }}
                </span>
                <span
                  v-if="hasAcknowledgedRisk"
                  class="inline-flex items-center gap-1 rounded-full bg-[var(--md-sys-color-tertiary-container)] text-[var(--md-sys-color-on-tertiary-container)] px-2.5 py-0.5 m3-label-large text-xs"
                >
                  <M3Icon
                    name="check_circle"
                    :filled="true"
                    :size="14"
                  />
                  {{ t('googleAuth.confirmed') }}
                </span>
              </div>
              <AccountRiskCard v-model="hasAcknowledgedRisk" />
            </div>

            <!-- SUCCESS STATE: Visual Confirmation (fully M3 tokenized) -->
            <div
              v-if="verifiedUser"
              class="flex flex-col items-center justify-center gap-4 rounded-[20px] bg-[var(--md-sys-color-surface-container)] p-6 text-center animate-fade-in"
            >
              <div class="relative">
                <div
                  class="flex size-20 items-center justify-center rounded-full m3-headline-large shadow-md ring-4 ring-[var(--md-sys-color-tertiary-container)]"
                  :style="{
                    backgroundColor: getAvatarColor(verifiedUser.email).bg,
                    color: getAvatarColor(verifiedUser.email).text,
                  }"
                >
                  {{ getEmailInitial(verifiedUser.email) }}
                </div>
                <div class="absolute -bottom-1 -right-1 flex size-7 items-center justify-center rounded-full bg-[var(--md-sys-color-tertiary)] text-[var(--md-sys-color-on-tertiary)] shadow-md">
                  <M3Icon
                    name="check"
                    :size="16"
                  />
                </div>
              </div>

              <div class="flex flex-col gap-1">
                <h3 class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                  {{ displayNameFromEmail(verifiedUser.email) }}
                </h3>
                <p class="m3-body-medium m3-mono text-[var(--md-sys-color-on-surface-variant)]">
                  {{ verifiedUser.email }}
                </p>
                <div class="mt-2 inline-flex items-center gap-1.5 rounded-full bg-[var(--md-sys-color-tertiary-container)] px-3 py-1 m3-label-large text-[var(--md-sys-color-on-tertiary-container)] mx-auto">
                  <M3Icon
                    name="verified"
                    :filled="true"
                    :size="15"
                  />
                  <span>{{ t('googleAuth.verified') }}</span>
                </div>
                <p
                  v-if="browserCloseFallbackMessage"
                  class="mt-2 m3-body-medium text-[var(--md-sys-color-primary)] text-center animate-fade-in"
                >
                  {{ browserCloseFallbackMessage }}
                </p>
              </div>
            </div>

            <!-- Step 2: Sign-in Options (Gated until risk is acknowledged) -->
            <div
              v-else
              class="flex flex-col gap-4 transition-opacity duration-200"
              :class="!hasAcknowledgedRisk && 'opacity-40 pointer-events-none select-none'"
            >
              <div class="flex items-center justify-between px-1">
                <span class="m3-label-large text-[var(--md-sys-color-on-surface-variant)]">
                  {{ t('googleAuth.step2Title') }}
                </span>
              </div>

              <!-- Primary In-App Google Sign-in Card -->
              <div class="flex flex-col gap-3 rounded-[20px] bg-[var(--md-sys-color-surface-container)] p-4">
                <!-- State: IDLE -->
                <div
                  v-if="browserState === 'idle'"
                  class="flex flex-col gap-3"
                >
                  <div class="flex flex-col gap-1">
                    <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      {{ t('googleAuth.automatedSignIn') }}
                    </span>
                    <span class="m3-body-medium leading-relaxed text-[var(--md-sys-color-on-surface-variant)]">
                      {{ t('googleAuth.automatedSignInDesc') }}
                    </span>
                  </div>

                  <M3Button
                    variant="filled"
                    size="m"
                    icon="login"
                    class="w-full justify-center shadow-xs"
                    :disabled="!hasAcknowledgedRisk || isValidating"
                    @click="handleEmbeddedSignIn"
                  >
                    <span>{{ t('googleAuth.signInWithBrowser') }}</span>
                  </M3Button>
                </div>

                <!-- State: WAITING (M3E Loading Indicator) -->
                <div
                  v-else-if="browserState === 'waiting' || browserState === 'waiting-external'"
                  class="flex flex-col items-center justify-center gap-3 py-3 text-center"
                >
                  <div class="flex size-12 items-center justify-center rounded-[16px] bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                    <M3LoadingIndicator size="sm" />
                  </div>

                  <div class="flex flex-col gap-1">
                    <h4 class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      {{ t('googleAuth.finishInBrowser') }}
                    </h4>
                    <p class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)] max-w-sm">
                      {{ t('googleAuth.finishInBrowserDesc') }}
                    </p>
                  </div>

                  <div class="flex flex-wrap items-center justify-center gap-2 mt-1">
                    <M3Button
                      variant="tonal"
                      size="s"
                      icon="close"
                      @click="handleCancelBrowserSignIn"
                    >
                      <span>{{ t('googleAuth.cancelSignIn') }}</span>
                    </M3Button>
                    <M3Button
                      v-if="browserState === 'waiting' && hasExternalChromium"
                      variant="text"
                      size="s"
                      icon="open_in_browser"
                      @click="handleExternalBrowserSignIn"
                    >
                      <span>{{ t('googleAuth.externalBrowserFallback', { browser: detectedBrowserName || 'Chrome' }) }}</span>
                    </M3Button>
                  </div>
                </div>

                <!-- State: ERROR (clean tonal error-container, >= 40px action targets) -->
                <div
                  v-else-if="browserState === 'error'"
                  class="flex flex-col gap-3 rounded-[16px] bg-[var(--md-sys-color-error-container)] text-[var(--md-sys-color-on-error-container)] p-4"
                >
                  <div class="flex items-start gap-2.5">
                    <M3Icon
                      name="error"
                      :filled="true"
                      :size="20"
                      class="shrink-0 mt-0.5"
                    />
                    <div class="flex flex-col gap-0.5 min-w-0">
                      <span class="m3-title-medium !text-[var(--md-sys-color-on-error-container)]">
                        {{ t('googleAuth.couldNotSignIn') }}
                      </span>
                      <span class="m3-body-medium !text-[var(--md-sys-color-on-error-container)] leading-relaxed">
                        {{ browserErrorMessage }}
                      </span>
                    </div>
                  </div>

                  <div class="flex flex-wrap items-center gap-2 pt-1">
                    <M3Button
                      variant="filled"
                      size="s"
                      icon="refresh"
                      @click="handleEmbeddedSignIn"
                    >
                      <span>{{ t('googleAuth.tryAgain') }}</span>
                    </M3Button>
                    <M3Button
                      v-if="hasExternalChromium"
                      variant="tonal"
                      size="s"
                      icon="open_in_browser"
                      @click="handleExternalBrowserSignIn"
                    >
                      <span>{{ t('googleAuth.signInWithExternalBrowser', { browser: detectedBrowserName || 'Chrome' }) }}</span>
                    </M3Button>
                    <M3Button
                      variant="text"
                      size="s"
                      icon="key"
                      @click="handleFallbackToManual"
                    >
                      <span>{{ t('googleAuth.enterTokenManually') }}</span>
                    </M3Button>
                  </div>
                </div>
              </div>

              <!-- Secondary Fallback Links (Manual + External Browser) -->
              <div
                v-if="browserState === 'idle'"
                class="flex flex-wrap items-center justify-between gap-2 px-1"
              >
                <M3Button
                  variant="text"
                  size="s"
                  :icon="showManualFallback ? 'expand_less' : 'expand_more'"
                  @click="showManualFallback = !showManualFallback"
                >
                  {{ showManualFallback ? t('googleAuth.hideManual') : t('googleAuth.showManual') }}
                </M3Button>

                <div class="flex items-center gap-1">
                  <M3Button
                    v-if="!showManualFallback && hasExternalChromium"
                    variant="text"
                    size="s"
                    icon="open_in_browser"
                    class="shrink-0 whitespace-nowrap"
                    @click="handleExternalBrowserSignIn"
                  >
                    <span>{{ t('googleAuth.signInWithExternalBrowser', { browser: detectedBrowserName || 'Chrome' }) }}</span>
                  </M3Button>
                  <M3Button
                    v-if="showManualFallback"
                    variant="text"
                    size="s"
                    icon="content_paste"
                    class="shrink-0 whitespace-nowrap"
                    @click="pasteFromClipboard"
                  >
                    <span>{{ t('googleAuth.pasteClipboard') }}</span>
                  </M3Button>
                </div>
              </div>

              <!-- MANUAL FORM (Visible if showManualFallback is true) -->
              <div
                v-if="showManualFallback"
                class="flex flex-col gap-4 animate-fade-in"
              >
                <!-- M3E Connected Button Group Tabs -->
                <M3ButtonGroup
                  :connected="true"
                  class="w-full"
                >
                  <M3Button
                    size="s"
                    :toggle="true"
                    :selected="authMode === 'oauth'"
                    :variant="authMode === 'oauth' ? 'tonal' : 'outlined'"
                    icon="open_in_browser"
                    class="flex-1 justify-center"
                    @click="authMode = 'oauth'"
                  >
                    <span>{{ t('googleAuth.tabBrowser') }}</span>
                  </M3Button>

                  <M3Button
                    size="s"
                    :toggle="true"
                    :selected="authMode === 'direct'"
                    :variant="authMode === 'direct' ? 'tonal' : 'outlined'"
                    icon="terminal"
                    class="flex-1 justify-center"
                    @click="authMode = 'direct'"
                  >
                    <span>{{ t('googleAuth.tabDirect') }}</span>
                  </M3Button>
                </M3ButtonGroup>

                <!-- Option A: Browser Sign-in Tab -->
                <div
                  v-if="authMode === 'oauth'"
                  class="flex flex-col gap-3 rounded-[20px] bg-[var(--md-sys-color-surface-container)] p-4"
                >
                  <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3">
                    <div class="flex flex-col">
                      <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                        {{ t('googleAuth.step1Browser') }}
                      </span>
                      <span class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)]">
                        {{ t('googleAuth.step1BrowserDesc') }}
                      </span>
                    </div>
                    <M3Button
                      variant="tonal"
                      size="s"
                      icon="open_in_new"
                      class="shrink-0 whitespace-nowrap"
                      @click="openEmbeddedSetup"
                    >
                      <span>{{ t('googleAuth.openSignIn') }}</span>
                    </M3Button>
                  </div>

                  <div class="rounded-[16px] bg-[var(--md-sys-color-surface-container-low)] p-3.5 m3-body-medium text-[var(--md-sys-color-on-surface-variant)] leading-relaxed">
                    <ol class="list-decimal space-y-1 pl-4">
                      <li>{{ t('googleAuth.instr1') }} <strong class="text-[var(--md-sys-color-on-surface)]">{{ t('googleAuth.instr1Bold') }}</strong>.</li>
                      <li>{{ t('googleAuth.instr2') }}</li>
                      <li>{{ t('googleAuth.instr3') }}</li>
                    </ol>
                  </div>

                  <div class="relative w-full pt-1">
                    <M3TextField
                      id="google-oauth-token"
                      ref="tokenFieldRef"
                      v-model="oauthToken"
                      type="password"
                      :label="t('googleAuth.cookieLabel')"
                      :placeholder="t('googleAuth.cookiePlaceholder')"
                      variant="outlined"
                      leading-icon="key"
                      :disabled="isValidating"
                      @keydown.enter="handleConnect"
                    />
                    <M3Button
                      variant="text"
                      size="s"
                      icon="content_paste"
                      :aria-label="t('googleAuth.pasteClipboard')"
                      :title="t('googleAuth.pasteClipboard')"
                      class="!absolute right-1.5 top-2.5 !size-10 !min-w-0 !p-0"
                      @click="pasteFromClipboard"
                    />
                  </div>
                </div>

                <!-- Option B: Direct Token / Credential Tab -->
                <div
                  v-else
                  class="flex flex-col gap-3 rounded-[20px] bg-[var(--md-sys-color-surface-container)] p-4"
                >
                  <div class="flex flex-col">
                    <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">
                      {{ t('googleAuth.directTitle') }}
                    </span>
                    <span class="m3-body-medium text-[var(--md-sys-color-on-surface-variant)] mt-0.5 leading-relaxed">
                      {{ t('googleAuth.directDesc') }}
                    </span>
                  </div>

                  <div class="relative w-full">
                    <M3TextField
                      id="google-raw-credential"
                      v-model="rawCredential"
                      type="password"
                      :label="t('googleAuth.directLabel')"
                      :placeholder="t('googleAuth.directPlaceholder')"
                      variant="outlined"
                      leading-icon="terminal"
                      :disabled="isValidating"
                      @keydown.enter="handleConnect"
                    />
                    <M3Button
                      variant="text"
                      size="s"
                      icon="content_paste"
                      :aria-label="t('googleAuth.pasteClipboard')"
                      :title="t('googleAuth.pasteClipboard')"
                      class="!absolute right-1.5 top-2 !size-10 !min-w-0 !p-0"
                      @click="pasteFromClipboard"
                    />
                  </div>
                </div>
              </div>

              <!-- Validation Error Banner (tonal error-container + on-error-container) -->
              <div
                v-if="validationError"
                class="flex items-start gap-3 rounded-[16px] bg-[var(--md-sys-color-error-container)] px-4 py-3.5 m3-body-medium text-[var(--md-sys-color-on-error-container)]"
                role="alert"
              >
                <M3Icon
                  name="error"
                  :filled="true"
                  :size="20"
                  class="shrink-0 mt-0.5"
                />
                <div class="flex flex-col min-w-0">
                  <span class="m3-title-medium !text-[var(--md-sys-color-on-error-container)]">{{ t('googleAuth.validationFailed') }}</span>
                  <span class="m3-body-medium !text-[var(--md-sys-color-on-error-container)] leading-relaxed">{{ validationError }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Sticky M3E Footer -->
          <div class="sticky bottom-0 z-20 shrink-0 bg-[var(--md-sys-color-surface-container-high)] px-6 pt-4 pb-6 flex items-center justify-end gap-2">
            <template v-if="verifiedUser">
              <M3Button
                variant="filled"
                size="m"
                class="w-full"
                @click="handleDoneSuccess"
              >
                <span>{{ t('googleAuth.done') }}</span>
              </M3Button>
            </template>
            <template v-else>
              <M3Button
                variant="text"
                size="s"
                :disabled="isValidating"
                @click="closeDialog"
              >
                <span>{{ t('googleAuth.cancel') }}</span>
              </M3Button>
              <M3Button
                variant="filled"
                size="m"
                class="min-w-36 shrink-0 whitespace-nowrap"
                :disabled="!showManualFallback || browserState !== 'idle' || !isFormValid || isValidating"
                :loading="isValidating"
                :aria-label="t('googleAuth.verifyConnect')"
                @click="handleConnect"
              >
                <span>{{ isValidating ? t('googleAuth.verifying') : t('googleAuth.verifyConnect') }}</span>
              </M3Button>
            </template>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
