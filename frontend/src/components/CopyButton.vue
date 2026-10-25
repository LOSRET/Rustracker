<script setup lang="ts">
import { useClipboard } from "@vueuse/core"
import { useI18n } from "../composables/useI18n"

defineOptions({ inheritAttrs: false })

const props = defineProps<{
  value: string
  copyLabel: string
}>()

const { t } = useI18n()
const { copied, copy } = useClipboard({ copiedDuring: 1500, legacy: true })

function doCopy() {
  copy(props.value)
}
</script>

<template>
  <UTooltip :text="t('copied')" :open="copied" :content="{ side: 'top', sideOffset: 8 }">
    <button
      v-bind="$attrs"
      type="button"
      :class="[
        'cursor-pointer transition-colors focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1',
        copied ? 'text-good' : 'text-ink hover:text-accent',
      ]"
      :aria-label="`${value} (${copied ? t('copied') : copyLabel})`"
      @click="doCopy"
    >
      <slot>{{ value }}</slot>
    </button>
  </UTooltip>
</template>
