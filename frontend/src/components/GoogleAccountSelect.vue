<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import M3Icon from './ui/M3Icon.vue'
import M3Button from './ui/M3Button.vue'
import { displayNameFromEmail } from '../utils/avatar'
import {
  SelectItem as SelectItemPrimitive,
  SelectItemIndicator,
  SelectItemText,
  SelectTrigger as SelectTriggerPrimitive,
} from 'reka-ui'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectSeparator,
} from '@/components/ui/select'
import { vRipple } from '@/directives/ripple'

interface Props {
  modelValue?: string
  options?: string[]
  removingAccount?: string
  avatarUrl?: string | null
}

interface Emits {
  (event: 'update:modelValue', value: string): void
  (event: 'item-removed', value: string): void
  (event: 'add'): void
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: '',
  options: () => [],
  removingAccount: '',
  avatarUrl: null,
})

const emit = defineEmits<Emits>()
const isOpen = ref(false)
const selectedValue = ref(props.modelValue)
const triggerRef = ref<{ $el?: HTMLElement } | HTMLElement | null>(null)

function onRestoreFocus(e: Event) {
  e.preventDefault()
  const el = (triggerRef.value && '$el' in triggerRef.value && triggerRef.value.$el)
    ? triggerRef.value.$el
    : (triggerRef.value as HTMLElement | null)
  el?.focus({ preventScroll: true })
}

watch(() => props.modelValue, (newValue) => {
  if (newValue !== selectedValue.value) {
    selectedValue.value = newValue || ''
  }
})

watch(selectedValue, (newValue) => {
  emit('update:modelValue', newValue)
})

const selectedInitial = computed(() => {
  if (!selectedValue.value) return ''
  return selectedValue.value.trim().charAt(0).toUpperCase()
})
</script>

<template>
  <Select
    v-model="selectedValue"
    v-model:open="isOpen"
  >
    <SelectTriggerPrimitive
      ref="triggerRef"
      v-ripple
      class="m3-interactive m3-chip inline-flex h-9 max-w-[240px] items-center gap-2 rounded-full border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] pl-1.5 pr-3 m3-label-large text-[var(--md-sys-color-on-surface)] shadow-xs outline-none transition-colors duration-150 select-none cursor-pointer"
      style="--wails-draggable: none"
    >
      <img
        v-if="avatarUrl"
        :src="avatarUrl"
        alt="Account avatar"
        class="size-6 shrink-0 rounded-full object-cover"
      >
      <div
        v-else-if="selectedInitial"
        class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] m3-label-small text-[var(--md-sys-color-on-primary-container)]"
      >
        {{ selectedInitial }}
      </div>
      <div
        v-else
        class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface-variant)]"
      >
        <M3Icon
          name="person"
          :size="14"
        />
      </div>

      <span class="truncate m3-label-large">
        {{ selectedValue ? displayNameFromEmail(selectedValue) : 'Select account' }}
      </span>

      <M3Icon
        name="expand_more"
        :size="16"
        class="shrink-0 text-[var(--md-sys-color-on-surface-variant)] transition-transform"
        :style="{
          transitionDuration: 'var(--md-sys-motion-duration-spatial)',
          transitionTimingFunction: 'var(--md-sys-motion-easing-spatial)',
        }"
        :class="isOpen && 'rotate-180'"
      />
    </SelectTriggerPrimitive>

    <SelectContent
      align="start"
      position="popper"
      class="m3-select-content min-w-[var(--reka-select-trigger-width)] max-w-[248px] rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/50 p-1.5"
      style="--wails-draggable: none"
      @close-auto-focus="onRestoreFocus"
    >
      <SelectGroup class="flex flex-col gap-1">
        <div
          v-for="option in options"
          :key="option"
          class="flex w-full min-h-[48px] items-center rounded-2xl"
        >
          <SelectItemPrimitive
            :value="option"
            class="m3-interactive m3-menu-item flex flex-1 min-w-0 items-center min-h-[48px] gap-2.5 rounded-2xl pl-2.5 pr-1 py-1.5 m3-body-medium text-[var(--md-sys-color-on-surface)] data-[state=checked]:bg-[var(--md-sys-color-secondary-container)] data-[state=checked]:text-[var(--md-sys-color-on-secondary-container)] outline-none select-none cursor-pointer transition-colors"
          >
            <div
              class="flex size-7 shrink-0 items-center justify-center rounded-full m3-title-medium transition-colors"
              :class="option === selectedValue
                ? 'bg-[var(--md-sys-color-on-secondary-container)] text-[var(--md-sys-color-secondary-container)]'
                : 'bg-[var(--md-sys-color-primary-container)] text-[var(--md-sys-color-on-primary-container)]'"
            >
              {{ option.charAt(0).toUpperCase() }}
            </div>
            <SelectItemText class="min-w-0 flex-1 truncate">
              <div class="flex flex-col min-w-0 text-left">
                <span class="block truncate m3-title-medium leading-tight">{{ displayNameFromEmail(option) }}</span>
                <span class="block truncate m3-body-medium leading-tight">{{ option }}</span>
              </div>
            </SelectItemText>
            <SelectItemIndicator class="shrink-0 text-[var(--md-sys-color-on-secondary-container)] mx-0.5">
              <M3Icon
                name="check"
                :size="16"
              />
            </SelectItemIndicator>
          </SelectItemPrimitive>
          <M3Button
            variant="text"
            size="xs"
            destructive
            icon="close"
            :aria-label="`Remove ${option}`"
            :title="`Remove ${option}`"
            :disabled="removingAccount === option"
            class="!size-8 !min-w-0 !p-0 mr-1"
            @pointerdown.stop.prevent
            @click.stop.prevent="emit('item-removed', option)"
          />
        </div>
      </SelectGroup>
      <SelectSeparator
        v-if="options.length"
        class="my-1.5 h-px bg-[var(--md-sys-color-outline-variant)]/40"
      />
      <M3Button
        variant="text"
        size="s"
        icon="add"
        full-width
        class="!justify-start"
        @pointerdown.stop.prevent
        @click.stop.prevent="isOpen = false; emit('add')"
      >
        Add account
      </M3Button>
    </SelectContent>
  </Select>
</template>
