<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Check, ChevronDown, Plus, User, X } from '@lucide/vue'
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
})

const emit = defineEmits<Emits>()
const isOpen = ref(false)
const selectedValue = ref(props.modelValue)

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
      v-ripple
      class="m3-interactive m3-chip inline-flex h-9 max-w-[240px] items-center gap-2 rounded-full border border-[var(--md-sys-color-outline-variant)]/40 bg-[var(--md-sys-color-surface-container-high)] pl-1.5 pr-3 text-xs font-medium text-[var(--md-sys-color-on-surface)] shadow-xs outline-none transition-all focus-visible:ring-2 focus-visible:ring-[var(--md-sys-color-primary)] select-none cursor-pointer"
      style="--wails-draggable: none"
    >
      <div
        v-if="selectedInitial"
        class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[11px] font-semibold text-[var(--md-sys-color-on-primary-container)]"
      >
        {{ selectedInitial }}
      </div>
      <div
        v-else
        class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-surface-container-highest)] text-[var(--md-sys-color-on-surface-variant)]"
      >
        <User class="size-3.5" />
      </div>

      <span class="truncate text-xs font-medium">
        {{ selectedValue || 'Select account' }}
      </span>

      <ChevronDown
        class="size-3.5 shrink-0 text-[var(--md-sys-color-on-surface-variant)] transition-transform"
        :style="{
          transitionDuration: 'var(--md-sys-motion-duration-spatial)',
          transitionTimingFunction: 'var(--md-sys-motion-easing-spatial)',
        }"
        :class="isOpen && 'rotate-180'"
      />
    </SelectTriggerPrimitive>

    <SelectContent
      align="center"
      class="min-w-[220px] max-w-[min(18rem,calc(100vw-3rem))] rounded-[24px] border border-[var(--md-sys-color-outline-variant)]/50 bg-[var(--md-sys-color-surface-container-high)] p-1.5 shadow-2xl backdrop-blur-md"
      style="--wails-draggable: none"
    >
      <SelectGroup>
        <div
          v-for="option in options"
          :key="option"
          class="relative flex items-center"
        >
          <SelectItemPrimitive
            :value="option"
            class="relative flex w-full min-w-0 cursor-pointer items-center gap-2 rounded-xl py-2 pr-8 pl-2.5 text-xs text-[var(--md-sys-color-on-surface)] outline-none select-none transition-colors hover:bg-[var(--md-sys-color-surface-container-highest)] focus:bg-[var(--md-sys-color-surface-container-highest)]"
          >
            <div class="flex size-6 shrink-0 items-center justify-center rounded-full bg-[var(--md-sys-color-primary-container)] text-[11px] font-semibold text-[var(--md-sys-color-on-primary-container)]">
              {{ option.charAt(0).toUpperCase() }}
            </div>
            <SelectItemText class="min-w-0 flex-1">
              <span class="block truncate">{{ option }}</span>
            </SelectItemText>
            <SelectItemIndicator class="shrink-0 text-[var(--md-sys-color-primary)] mr-1">
              <Check class="size-3.5" />
            </SelectItemIndicator>
          </SelectItemPrimitive>
          <button
            type="button"
            class="absolute right-1 top-1/2 z-10 flex size-6 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full text-[var(--md-sys-color-on-surface-variant)] transition-colors hover:bg-[var(--md-sys-color-error-container)] hover:text-[var(--md-sys-color-on-error-container)] disabled:pointer-events-none disabled:opacity-40"
            :aria-label="`Remove ${option}`"
            :title="`Remove ${option}`"
            :disabled="removingAccount === option"
            @pointerdown.stop.prevent
            @click.stop.prevent="emit('item-removed', option)"
          >
            <X class="size-3" />
          </button>
        </div>
      </SelectGroup>
      <SelectSeparator
        v-if="options.length"
        class="my-1 h-px bg-[var(--md-sys-color-outline-variant)]/40"
      />
      <button
        type="button"
        class="flex w-full cursor-pointer items-center gap-2 rounded-xl py-2 px-3 text-xs font-medium text-[var(--md-sys-color-primary)] outline-none select-none transition-colors hover:bg-[var(--md-sys-color-primary-container)]/30"
        @pointerdown.stop.prevent
        @click.stop.prevent="isOpen = false; emit('add')"
      >
        <Plus class="size-4" />
        Add account
      </button>
    </SelectContent>
  </Select>
</template>
