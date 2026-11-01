<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import M3Icon from './ui/M3Icon.vue'
import {
  displayNameFromEmail,
  getAvatarColor,
  getEmailInitial,
} from '../utils/avatar'

interface Props {
  open: boolean
  anchorEl: HTMLElement | null
  activeAccount: string
  accounts: string[]
  customAvatar?: string | null
}

const props = withDefaults(defineProps<Props>(), {
  customAvatar: null,
})

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'switch-account', email: string): void
  (e: 'manage-accounts'): void
  (e: 'cycle-theme'): void
  (e: 'upload-avatar'): void
}>()

const menuRef = ref<HTMLElement | null>(null)
const menuPosition = ref<{ left: number; bottom: number }>({ left: 88, bottom: 8 })

function calculatePosition() {
  if (!props.anchorEl) return

  const triggerRect = props.anchorEl.getBoundingClientRect()
  const rail = props.anchorEl.closest('nav')
  const railRect = rail ? rail.getBoundingClientRect() : null

  // Menu left edge = rail right edge + 8px (fallback to triggerRect.right + 8px)
  const railRight = railRect ? railRect.right : triggerRect.right
  let left = railRight + 8

  // Bottom edge aligns with the avatar trigger's bottom edge
  let bottom = window.innerHeight - triggerRect.bottom

  // Measure or estimate menu dimensions
  const menuWidth = menuRef.value?.offsetWidth || 288
  const menuHeight = menuRef.value?.offsetHeight || 280

  // Viewport collision handling (keep at least 8px from every window edge)
  if (left + menuWidth > window.innerWidth - 8) {
    left = Math.max(8, window.innerWidth - 8 - menuWidth)
  }
  if (left < 8) {
    left = 8
  }

  // Top collision: ensure top edge of menu is at least 8px from window top
  if (window.innerHeight - bottom - menuHeight < 8) {
    bottom = Math.max(8, window.innerHeight - menuHeight - 8)
  }
  // Bottom collision: ensure bottom edge is at least 8px from window bottom
  if (bottom < 8) {
    bottom = 8
  }

  menuPosition.value = { left, bottom }
}

function getFocusableElements(): HTMLElement[] {
  if (!menuRef.value) return []
  return Array.from(
    menuRef.value.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [role="button"]:not([disabled]), [tabindex="0"]:not([disabled])'
    )
  )
}

function focusFirstItem() {
  const items = getFocusableElements()
  if (items.length > 0) {
    items[0].focus()
  } else {
    menuRef.value?.focus()
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
    props.anchorEl?.focus()
    return
  }

  const items = getFocusableElements()
  if (items.length === 0) return
  const active = document.activeElement as HTMLElement
  const currentIndex = items.indexOf(active)

  if (e.key === 'ArrowDown') {
    e.preventDefault()
    const next = currentIndex < items.length - 1 ? currentIndex + 1 : 0
    items[next].focus()
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    const prev = currentIndex > 0 ? currentIndex - 1 : items.length - 1
    items[prev].focus()
  } else if (e.key === 'Home') {
    e.preventDefault()
    items[0].focus()
  } else if (e.key === 'End') {
    e.preventDefault()
    items[items.length - 1].focus()
  } else if (e.key === 'Tab') {
    if (e.shiftKey && currentIndex <= 0) {
      e.preventDefault()
      items[items.length - 1].focus()
    } else if (!e.shiftKey && currentIndex >= items.length - 1) {
      e.preventDefault()
      items[0].focus()
    }
  }
}

function handlePointerDown(e: MouseEvent | PointerEvent) {
  if (!props.open) return
  const target = e.target as Node | null
  if (!target) return
  if (menuRef.value?.contains(target)) return
  if (props.anchorEl?.contains(target)) return
  emit('close')
}

watch(
  () => props.open,
  async (isOpen, wasOpen) => {
    if (isOpen) {
      await nextTick()
      calculatePosition()
      requestAnimationFrame(() => {
        calculatePosition()
        focusFirstItem()
      })
    } else if (wasOpen) {
      props.anchorEl?.focus()
    }
  }
)

function handleGlobalKeydown(e: KeyboardEvent) {
  if (!props.open) return
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
    props.anchorEl?.focus()
  }
}

onMounted(() => {
  window.addEventListener('pointerdown', handlePointerDown, true)
  window.addEventListener('keydown', handleGlobalKeydown)
  window.addEventListener('resize', calculatePosition)
})

onUnmounted(() => {
  window.removeEventListener('pointerdown', handlePointerDown, true)
  window.removeEventListener('keydown', handleGlobalKeydown)
  window.removeEventListener('resize', calculatePosition)
})
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-account-overlay">
      <div
        v-if="open"
        ref="menuRef"
        class="m3-account-menu-overlay fixed z-[100] w-72 min-w-[260px] max-h-[calc(100vh-16px)] overflow-y-auto rounded-2xl border border-[var(--md-sys-color-outline-variant)] bg-[var(--md-sys-color-surface-container)] p-2 shadow-2xl backdrop-blur-md text-[var(--md-sys-color-on-surface)] select-none focus:outline-none"
        :style="{
          left: `${menuPosition.left}px`,
          bottom: `${menuPosition.bottom}px`,
        }"
        role="menu"
        aria-label="Account menu"
        tabindex="-1"
        @keydown="handleKeydown"
      >
        <!-- Active Account Header -->
        <div
          v-if="activeAccount"
          class="flex items-center gap-3 p-2 rounded-xl bg-[var(--md-sys-color-surface-container-high)]/60"
        >
          <div
            class="group/avatar relative size-10 shrink-0 cursor-pointer overflow-hidden rounded-full border border-[var(--md-sys-color-outline-variant)]"
            :style="{
              backgroundColor: customAvatar ? 'transparent' : getAvatarColor(activeAccount).bg,
              color: getAvatarColor(activeAccount).text,
            }"
            title="Click to change profile photo"
            role="button"
            tabindex="0"
            @click="emit('upload-avatar')"
            @keydown.enter="emit('upload-avatar')"
          >
            <img
              v-if="customAvatar"
              :src="customAvatar"
              alt="Profile avatar"
              class="size-full object-cover"
            >
            <span
              v-else
              class="flex size-full items-center justify-center m3-title-medium"
            >
              {{ getEmailInitial(activeAccount) }}
            </span>
            <div class="absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover/avatar:opacity-100 transition-opacity">
              <M3Icon
                name="photo_camera"
                :size="14"
                class="text-white"
              />
            </div>
          </div>

          <div class="flex flex-col min-w-0 flex-1">
            <span class="truncate m3-title-medium text-[var(--md-sys-color-on-surface)]">
              {{ displayNameFromEmail(activeAccount) }}
            </span>
            <span class="truncate m3-body-medium">
              {{ activeAccount }}
            </span>
          </div>
        </div>

        <div
          v-else
          class="flex flex-col items-center gap-1.5 p-3 text-center"
        >
          <span class="m3-title-medium text-[var(--md-sys-color-on-surface)]">No account connected</span>
          <span class="m3-body-medium">Sign in to begin uploading photos</span>
        </div>

        <!-- Switch Account List (if accounts exist) -->
        <div
          v-if="accounts.length > 0"
          class="mt-2 flex flex-col gap-0.5"
        >
          <div class="px-2 py-1 m3-label-small text-[var(--md-sys-color-on-surface-variant)]">
            Switch account
          </div>
          <div class="max-h-40 overflow-y-auto flex flex-col gap-0.5">
            <button
              v-for="opt in accounts"
              :key="opt"
              type="button"
              class="flex items-center justify-between gap-2 rounded-xl px-2.5 py-1.5 transition cursor-pointer text-left hover:bg-[var(--md-sys-color-surface-container-high)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
              :class="opt === activeAccount ? 'bg-[var(--md-sys-color-primary-container)]/40 m3-label-large text-[var(--md-sys-color-primary)]' : 'm3-body-medium text-[var(--md-sys-color-on-surface)]'"
              @click="emit('switch-account', opt)"
            >
              <div class="flex items-center gap-2 min-w-0">
                <div
                  class="size-6 shrink-0 rounded-full flex items-center justify-center m3-label-small"
                  :style="{
                    backgroundColor: getAvatarColor(opt).bg,
                    color: getAvatarColor(opt).text,
                  }"
                >
                  {{ getEmailInitial(opt) }}
                </div>
                <span class="truncate">{{ opt }}</span>
              </div>
              <M3Icon
                v-if="opt === activeAccount"
                name="check"
                :size="14"
                class="shrink-0 text-[var(--md-sys-color-primary)]"
              />
            </button>
          </div>
        </div>

        <div class="my-1.5 h-px bg-[var(--md-sys-color-outline-variant)]/40" />

        <!-- Quick Navigation to Accounts Page -->
        <button
          type="button"
          class="flex w-full items-center gap-2.5 rounded-xl px-2.5 py-2 m3-label-large text-[var(--md-sys-color-on-surface)] hover:bg-[var(--md-sys-color-surface-container-high)] transition cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          @click="emit('manage-accounts')"
        >
          <M3Icon
            name="manage_accounts"
            :size="16"
            class="text-[var(--md-sys-color-on-surface-variant)]"
          />
          <span>Manage accounts</span>
        </button>

        <!-- Quick Action: Cycle Theme Palette -->
        <button
          type="button"
          class="flex w-full items-center justify-between rounded-xl px-2.5 py-2 m3-label-large text-[var(--md-sys-color-on-surface)] hover:bg-[var(--md-sys-color-surface-container-high)] transition cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)]"
          title="Cycle theme palette"
          @click="emit('cycle-theme')"
        >
          <div class="flex items-center gap-2.5">
            <M3Icon
              name="palette"
              :size="16"
              class="text-[var(--md-sys-color-primary)]"
            />
            <span>Cycle theme palette</span>
          </div>
          <span class="m3-label-small text-[var(--md-sys-color-on-surface-variant)]">Next</span>
        </button>
      </div>
    </Transition>
  </Teleport>
</template>
