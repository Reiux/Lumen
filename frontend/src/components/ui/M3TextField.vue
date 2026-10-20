<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import M3Icon from './M3Icon.vue'

interface Props {
  modelValue?: string | number
  label?: string
  placeholder?: string
  type?: string
  variant?: 'outlined' | 'filled'
  leadingIcon?: string
  trailingIcon?: string
  supportingText?: string
  errorText?: string
  error?: boolean
  disabled?: boolean
  readonly?: boolean
  required?: boolean
  maxlength?: number
  id?: string
  name?: string
  autofocus?: boolean
  autocomplete?: string
  spellcheck?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: '',
  label: '',
  placeholder: '',
  type: 'text',
  variant: 'outlined',
  leadingIcon: undefined,
  trailingIcon: undefined,
  supportingText: undefined,
  errorText: undefined,
  error: false,
  disabled: false,
  readonly: false,
  required: false,
  maxlength: undefined,
  id: undefined,
  name: undefined,
  autofocus: false,
  autocomplete: 'off',
  spellcheck: false,
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
  (e: 'input', value: string): void
  (e: 'change', value: string): void
  (e: 'trailingIconClick'): void
  (e: 'keydown', event: KeyboardEvent): void
}>()

const inputRef = ref<HTMLInputElement | null>(null)
const isFocused = ref(false)

const hasValue = computed(() => {
  return props.modelValue !== undefined && props.modelValue !== null && String(props.modelValue).length > 0
})

const isFloating = computed(() => {
  return isFocused.value || hasValue.value
})

const effectiveError = computed(() => {
  return props.error || !!props.errorText
})

function onFocus() {
  if (props.disabled || props.readonly) return
  isFocused.value = true
}

function onBlur() {
  isFocused.value = false
}

function onInput(e: Event) {
  const val = (e.target as HTMLInputElement).value
  emit('update:modelValue', val)
  emit('input', val)
}

function onChange(e: Event) {
  const val = (e.target as HTMLInputElement).value
  emit('update:modelValue', val)
  emit('change', val)
}

function onKeyDown(e: KeyboardEvent) {
  emit('keydown', e)
}

function onTrailingClick() {
  emit('trailingIconClick')
}

onMounted(async () => {
  if (props.autofocus) {
    await nextTick()
    inputRef.value?.focus()
  }
})

watch(
  () => props.autofocus,
  (val) => {
    if (val) {
      inputRef.value?.focus()
    }
  }
)

defineExpose({
  focus: () => {
    inputRef.value?.focus()
  },
})
</script>

<template>
  <div class="m3-text-field-wrapper flex flex-col w-full text-left select-none">
    <!-- M3 Outlined Container (56dp height, 4dp corners, 1-2dp outline) -->
    <div
      class="relative flex items-center h-14 w-full rounded-[4px] transition-all duration-200 cursor-text"
      :class="[
        variant === 'filled'
          ? 'bg-[var(--md-sys-color-surface-container-highest)] border-b-2'
          : 'bg-transparent border',
        effectiveError
          ? 'border-[var(--md-sys-color-error)] ring-1 ring-[var(--md-sys-color-error)]'
          : isFocused
            ? 'border-2 border-[var(--md-sys-color-primary)]'
            : 'border-[var(--md-sys-color-outline-variant)] hover:border-[var(--md-sys-color-on-surface)]/60',
        disabled && 'opacity-38 pointer-events-none border-[var(--md-sys-color-outline-variant)]/40',
      ]"
      @click="inputRef?.focus()"
    >
      <!-- Leading Icon (24dp) -->
      <div
        v-if="leadingIcon"
        class="absolute left-3.5 flex items-center justify-center size-6 pointer-events-none transition-colors duration-200"
        :class="effectiveError
          ? 'text-[var(--md-sys-color-error)]'
          : isFocused
            ? 'text-[var(--md-sys-color-primary)]'
            : 'text-[var(--md-sys-color-on-surface-variant)]'"
      >
        <M3Icon
          :name="leadingIcon"
          :size="20"
        />
      </div>

      <!-- Floating Label (sitting in border cutout when active) -->
      <label
        v-if="label"
        :for="id"
        class="absolute pointer-events-none transition-all duration-200 ease-[var(--md-sys-motion-easing-spatial)] select-none truncate max-w-[calc(100%-32px)]"
        :class="[
          isFloating
            ? '-top-2.5 left-3 px-1 text-xs font-medium rounded-sm bg-[var(--md-sys-color-surface-container-high)] z-10'
            : leadingIcon
              ? 'left-11 top-4 text-sm'
              : 'left-4 top-4 text-sm',
          effectiveError
            ? 'text-[var(--md-sys-color-error)]'
            : isFocused
              ? 'text-[var(--md-sys-color-primary)]'
              : 'text-[var(--md-sys-color-on-surface-variant)]',
        ]"
      >
        {{ label }}
      </label>

      <!-- Native Input -->
      <input
        :id="id"
        ref="inputRef"
        :value="modelValue"
        :type="type"
        :placeholder="isFloating || !label ? placeholder : ''"
        :disabled="disabled"
        :readonly="readonly"
        :required="required"
        :maxlength="maxlength"
        :name="name"
        :autocomplete="autocomplete"
        :spellcheck="spellcheck"
        class="w-full h-full bg-transparent text-sm text-[var(--md-sys-color-on-surface)] placeholder:text-[var(--md-sys-color-on-surface-variant)]/60 outline-none border-0"
        :class="[
          leadingIcon ? 'pl-11' : 'pl-4',
          trailingIcon ? 'pr-11' : 'pr-4',
        ]"
        @focus="onFocus"
        @blur="onBlur"
        @input="onInput"
        @change="onChange"
        @keydown="onKeyDown"
      >

      <!-- Trailing Icon / Action -->
      <div
        v-if="trailingIcon"
        class="absolute right-3.5 flex items-center justify-center size-6 transition-colors duration-200 cursor-pointer"
        :class="effectiveError
          ? 'text-[var(--md-sys-color-error)]'
          : isFocused
            ? 'text-[var(--md-sys-color-primary)]'
            : 'text-[var(--md-sys-color-on-surface-variant)] hover:text-[var(--md-sys-color-on-surface)]'"
        @click.stop="onTrailingClick"
      >
        <M3Icon
          :name="trailingIcon"
          :size="20"
        />
      </div>
    </div>

    <!-- Supporting or Error Text -->
    <div
      v-if="errorText || supportingText"
      class="flex items-center justify-between px-3.5 pt-1 text-[11px] leading-tight"
      :class="effectiveError ? 'text-[var(--md-sys-color-error)] font-medium' : 'text-[var(--md-sys-color-on-surface-variant)]'"
    >
      <span class="truncate">{{ errorText || supportingText }}</span>
      <span
        v-if="maxlength"
        class="ml-2 shrink-0 opacity-70"
      >
        {{ String(modelValue || '').length }}/{{ maxlength }}
      </span>
    </div>
  </div>
</template>

<style scoped>
input:-webkit-autofill,
input:-webkit-autofill:hover,
input:-webkit-autofill:focus {
  -webkit-text-fill-color: var(--md-sys-color-on-surface);
  transition: background-color 5000s ease-in-out 0s;
}
</style>
