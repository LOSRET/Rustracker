<script setup lang="ts" generic="T extends string">
import { computed } from "vue"

const props = defineProps<{
  items: { label: string; value: T }[]
  size?: "sm" | "md"
}>()

const model = defineModel<T>({ required: true })

// URadioGroup's modelValue/items conditional types cannot resolve against an
// outer generic, so bridge both through string-typed computeds.
const modelValue = computed({
  get: () => model.value as string,
  set: (v: string) => {
    model.value = v as T
  },
})

const valueItems = computed(() => props.items.map((i) => ({ ...i, value: i.value as string })))

const itemClass = computed(() => {
  const sizing = props.size === "sm" ? "px-3 text-xs min-h-7" : "px-4 text-[13px] min-h-8"
  return [
    "border text-muted cursor-pointer transition-colors flex items-center justify-center bg-panel border-line hover:bg-hover-soft",
    "has-data-[state=checked]:bg-accent has-data-[state=checked]:border-accent has-data-[state=checked]:text-white has-data-[state=checked]:z-[1]",
    "first-of-type:rounded-s last-of-type:rounded-e",
    sizing,
  ].join(" ")
})
</script>

<template>
  <URadioGroup
    v-model="modelValue"
    :items="valueItems"
    variant="table"
    orientation="horizontal"
    indicator="hidden"
    color="neutral"
    :ui="{
      fieldset: () => 'flex shrink-0 gap-0 -space-x-px',
      item: () => itemClass,
      container: () => 'contents',
      wrapper: () => 'contents',
      label: () => 'text-inherit font-normal',
    }"
  />
</template>
