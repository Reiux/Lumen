<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { Browser, Clipboard } from '@wailsio/runtime'
import { toast } from 'vue-sonner'
import { ConfigManager } from '../../bindings/app/core'
import M3Button from './ui/M3Button.vue'
import M3Icon from './ui/M3Icon.vue'
import M3TextField from './ui/M3TextField.vue'

const embeddedSetupURL = 'https://accounts.google.com/EmbeddedSetup'

const emit = defineEmits<{
  (event: 'account-added'): void
}>()

const isOpen = defineModel<boolean>('open', { default: false })
const oauthToken = ref('')
const rawCredential = ref('')
const isConnecting = ref(false)
const isAddingRawCredential = ref(false)
const showAdvanced = ref(false)

// Visible validation & server errors
const tokenError = ref('')
const connectionError = ref('')
const rawCredentialError = ref('')
const dialogBodyRef = ref<HTMLElement | null>(null)
const tokenFieldRef = ref<{ focus?: () => void } | null>(null)

let openerElement: HTMLElement | null = null

watch(isOpen, (open) => {
  if (open) {
    openerElement = (document.activeElement as HTMLElement) || null
    nextTick(() => {
      if (dialogBodyRef.value) {
        dialogBodyRef.value.scrollTop = 0
      }
      tokenFieldRef.value?.focus?.()
    })
  } else {
    oauthToken.value = ''
    rawCredential.value = ''
    showAdvanced.value = false
    tokenError.value = ''
    connectionError.value = ''
    rawCredentialError.value = ''
    if (openerElement && typeof openerElement.focus === 'function') {
      openerElement.focus({ preventScroll: true })
    }
    openerElement = null
  }
})

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && isOpen.value) {
    e.stopPropagation()
    isOpen.value = false
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
})

watch(oauthToken, (val) => {
  if (val.trim()) {
    tokenError.value = ''
  }
  connectionError.value = ''
})

watch(rawCredential, (val) => {
  if (val.trim()) {
    rawCredentialError.value = ''
  }
})

async function openEmbeddedSetup() {
  try {
    await Browser.OpenURL(embeddedSetupURL)
  } catch (error) {
    toast.error('Could not open the system browser', {
      description: error instanceof Error ? error.message : String(error),
    })
  }
}

async function pasteToken() {
  try {
    let text = ''
    try {
      text = await Clipboard.Text()
    } catch {
      text = await navigator.clipboard.readText()
    }
    if (text) {
      oauthToken.value = text.trim()
      tokenError.value = ''
      toast.success('Pasted from clipboard')
    }
  } catch (err) {
    console.warn('Clipboard read failed:', err)
  }
}

async function pasteRawCredential() {
  try {
    let text = ''
    try {
      text = await Clipboard.Text()
    } catch {
      text = await navigator.clipboard.readText()
    }
    if (text) {
      rawCredential.value = text.trim()
      rawCredentialError.value = ''
      toast.success('Pasted from clipboard')
    }
  } catch (err) {
    console.warn('Clipboard read failed:', err)
  }
}

async function connectAccount() {
  const normalizedToken = oauthToken.value.trim()
  if (!normalizedToken) {
    tokenError.value = 'Please enter or paste the oauth_token cookie value.'
    return
  }

  isConnecting.value = true
  connectionError.value = ''
  try {
    const connectedEmail = await ConfigManager.AddGoogleAccount(normalizedToken)
    oauthToken.value = ''
    isOpen.value = false
    emit('account-added')
    toast.success('Google account connected.', {
      description: connectedEmail,
    })
  } catch (error) {
    const errMsg = error instanceof Error ? error.message : String(error)
    connectionError.value = errMsg || 'Could not connect Google account. Please verify the token.'
  } finally {
    isConnecting.value = false
  }
}

async function addRawCredential() {
  const credential = rawCredential.value.trim()
  if (!credential) {
    rawCredentialError.value = 'Please enter or paste a valid captured credential string.'
    return
  }

  isAddingRawCredential.value = true
  rawCredentialError.value = ''
  try {
    await ConfigManager.AddCredentials(credential)
    rawCredential.value = ''
    isOpen.value = false
    emit('account-added')
    toast.success('Credentials added.')
  } catch (error) {
    const errMsg = error instanceof Error ? error.message : String(error)
    rawCredentialError.value = errMsg || 'Could not add credentials. Check the captured format.'
  } finally {
    isAddingRawCredential.value = false
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog-modal">
      <div
        v-if="isOpen"
        class="m3-dialog-wrapper fixed inset-0 z-50 grid place-items-center bg-black/70 p-4 select-none overflow-y-auto"
        style="--wails-draggable: none"
        @click.self="isOpen = false"
      >
        <!-- Centered M3 Modal Dialog (28dp corners, level 3 elevation, surface-container-high) -->
        <div
          class="m3-dialog-card relative flex flex-col w-full max-w-lg rounded-[28px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-high)] shadow-2xl outline-none max-h-[calc(100dvh-32px)] overflow-hidden select-none"
          role="dialog"
          aria-modal="true"
          aria-labelledby="google-auth-title"
          style="--wails-draggable: none"
        >
          <!-- Sticky Header -->
          <div class="sticky top-0 z-20 flex shrink-0 items-center justify-between border-b border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-high)] px-6 py-4">
            <div class="flex items-center gap-3">
              <div class="flex size-10 shrink-0 items-center justify-center rounded-2xl bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]">
                <M3Icon
                  name="key"
                  :filled="true"
                  :size="20"
                />
              </div>
              <div class="flex flex-col">
                <h2
                  id="google-auth-title"
                  class="text-base font-bold tracking-tight text-[var(--md-sys-color-on-surface)]"
                >
                  Connect Google Photos
                </h2>
                <p class="text-xs text-[var(--md-sys-color-on-surface-variant)]">
                  Embedded setup sign-in & credential management
                </p>
              </div>
            </div>

            <button
              type="button"
              class="m3-interactive flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-surface-container-highest)] hover:text-[var(--md-sys-color-on-surface)]"
              aria-label="Close dialog"
              @click="isOpen = false"
            >
              <M3Icon
                name="close"
                :size="16"
              />
            </button>
          </div>

          <!-- Scrollable Body -->
          <div
            ref="dialogBodyRef"
            class="m3-dialog-body flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-6"
          >
            <!-- Step 1: Open Google Sign-in (Grouped List Style) -->
            <div class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 border border-[var(--md-sys-color-outline-variant)]/30">
              <div class="flex items-center gap-2.5">
                <span class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-primary)] text-xs font-bold text-[var(--md-sys-color-on-primary)] shadow-xs">
                  1
                </span>
                <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
                  Open the Google sign-in page
                </h3>
              </div>

              <!-- Tonal card instructions -->
              <div class="m3-instructions-card rounded-xl border border-[var(--md-sys-color-outline-variant)]/20 bg-[var(--md-sys-color-surface-container-low)] p-3 text-xs leading-relaxed text-[var(--md-sys-color-on-surface-variant)]">
                <ol class="list-decimal space-y-1.5 pl-4">
                  <li>
                    Click the button below to open Google's Embedded Setup page in your default browser.
                  </li>
                  <li>
                    Sign in with your account and click <strong class="text-[var(--md-sys-color-on-surface)]">I agree</strong>. (The page may keep loading with a spinner; that is expected.)
                  </li>
                  <li>
                    Open DevTools (<kbd class="rounded bg-[var(--md-sys-color-surface-container-highest)] px-1 font-mono text-[10px]">F12</kbd>), navigate to <strong class="text-[var(--md-sys-color-on-surface)]">Application</strong> &rarr; <strong class="text-[var(--md-sys-color-on-surface)]">Cookies</strong> &rarr; <code class="font-mono text-[11px] text-[var(--md-sys-color-on-surface)]">accounts.google.com</code>.
                  </li>
                  <li>
                    Copy the cookie value for <code class="rounded-full bg-[var(--md-sys-color-surface-container-highest)] px-2.5 py-0.5 font-mono text-[11px] font-semibold text-[var(--md-sys-color-primary)]">oauth_token</code>.
                  </li>
                </ol>
              </div>

              <!-- Tonal secondary button per M3E spec -->
              <M3Button
                variant="tonal"
                size="s"
                icon="open_in_new"
                :disabled="isConnecting"
                aria-label="Open Google sign-in page in default browser"
                @click="openEmbeddedSetup"
              >
                <span>Open Google Sign-in</span>
              </M3Button>
            </div>

            <!-- Step 2: Paste Token (Grouped List Style) -->
            <div class="flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 border border-[var(--md-sys-color-outline-variant)]/30">
              <div class="flex items-center gap-2.5">
                <span class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-primary)] text-xs font-bold text-[var(--md-sys-color-on-primary)] shadow-xs">
                  2
                </span>
                <h3 class="text-sm font-semibold text-[var(--md-sys-color-on-surface)]">
                  Paste the oauth_token cookie value
                </h3>
              </div>

              <!-- M3 Outlined Text Field with 4dp corners & floating label -->
              <div class="relative w-full">
                <M3TextField
                  id="google-oauth-token"
                  ref="tokenFieldRef"
                  v-model="oauthToken"
                  type="password"
                  label="oauth_token Cookie Value"
                  variant="outlined"
                  leading-icon="key"
                  :error="!!tokenError"
                  :error-text="tokenError"
                  :disabled="isConnecting"
                  @keydown.enter="connectAccount"
                />
                <!-- Paste Action Icon Button overlay on trailing area -->
                <button
                  type="button"
                  class="m3-interactive absolute right-2 top-2.5 flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-surface-container-highest)] hover:text-[var(--md-sys-color-on-surface)]"
                  aria-label="Paste oauth_token from clipboard"
                  title="Paste from clipboard"
                  @click="pasteToken"
                >
                  <M3Icon
                    name="content_paste"
                    :size="18"
                  />
                </button>
              </div>

              <!-- Error-container Banner for Server Failures -->
              <div
                v-if="connectionError"
                class="flex items-start gap-2.5 rounded-xl border border-[var(--md-sys-color-error)]/50 bg-[var(--md-sys-color-error-container)] p-3 text-xs text-[var(--md-sys-color-on-error-container)] shadow-xs"
                role="alert"
              >
                <M3Icon
                  name="warning"
                  :filled="true"
                  :size="16"
                  class="shrink-0 mt-0.5 text-[var(--md-sys-color-error)]"
                />
                <div class="flex flex-col min-w-0">
                  <span class="font-bold">Connection Failed</span>
                  <span class="text-[11px] leading-relaxed opacity-95">{{ connectionError }}</span>
                </div>
              </div>
            </div>

            <!-- Collapsible Section: Paste a captured credential instead -->
            <div class="border-t border-[var(--md-sys-color-outline-variant)]/30 pt-4">
              <button
                type="button"
                class="flex w-full cursor-pointer items-center justify-between text-xs font-semibold text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:text-[var(--md-sys-color-on-surface)]"
                aria-label="Toggle captured credential input section"
                :aria-expanded="showAdvanced"
                @click="showAdvanced = !showAdvanced"
              >
                <span>Paste a captured credential instead</span>
                <M3Icon
                  name="expand_more"
                  :size="18"
                  class="transition-transform duration-200"
                  :class="showAdvanced && 'rotate-180'"
                />
              </button>

              <div
                v-if="showAdvanced"
                class="mt-4 flex flex-col gap-3 rounded-2xl bg-[var(--md-sys-color-surface-container)] p-4 border border-[var(--md-sys-color-outline-variant)]/30"
              >
                <div class="relative w-full">
                  <M3TextField
                    id="google-raw-credential"
                    v-model="rawCredential"
                    type="password"
                    label="Captured credential (androidId=...&Email=...)"
                    variant="outlined"
                    leading-icon="key"
                    :error="!!rawCredentialError"
                    :error-text="rawCredentialError"
                    :disabled="isAddingRawCredential"
                    @keydown.enter="addRawCredential"
                  />
                  <button
                    type="button"
                    class="m3-interactive absolute right-2 top-2.5 flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-surface-container-highest)] hover:text-[var(--md-sys-color-on-surface)]"
                    aria-label="Paste raw credential from clipboard"
                    title="Paste from clipboard"
                    @click="pasteRawCredential"
                  >
                    <M3Icon
                      name="content_paste"
                      :size="18"
                    />
                  </button>
                </div>

                <M3Button
                  variant="outlined"
                  size="s"
                  :disabled="!rawCredential.trim() || isAddingRawCredential"
                  :loading="isAddingRawCredential"
                  aria-label="Add captured credential"
                  @click="addRawCredential"
                >
                  <span>{{ isAddingRawCredential ? 'Adding...' : 'Add Credential' }}</span>
                </M3Button>
              </div>
            </div>
          </div>

          <!-- Sticky Footer with Filled Primary Connect Button per M3E spec -->
          <div class="sticky bottom-0 z-20 shrink-0 border-t border-[var(--md-sys-color-outline-variant)]/30 bg-[var(--md-sys-color-surface-container-high)] px-6 py-4 flex items-center justify-end">
            <M3Button
              variant="filled"
              size="m"
              class="w-full"
              :disabled="!oauthToken.trim() || isConnecting"
              :loading="isConnecting"
              aria-label="Connect Google account"
              @click="connectAccount"
            >
              <span>{{ isConnecting ? 'Connecting...' : 'Connect Account' }}</span>
            </M3Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
