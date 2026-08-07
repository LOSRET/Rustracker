<script setup lang="ts">
import { CN, GB, JP, RU, DE, UA } from "country-flag-icons/string/3x2"
import type { LangKey } from "../types/api"
import { useI18n } from "../composables/useI18n"

const { lang, setLang } = useI18n()

const flags: Record<LangKey, string> = { zh: CN, en: GB, ja: JP, ru: RU, de: DE, uk: UA }

const langs: { key: LangKey; label: string }[] = [
  { key: "zh", label: "中文" },
  { key: "en", label: "English" },
  { key: "ja", label: "日本語" },
  { key: "ru", label: "Русский" },
  { key: "de", label: "Deutsch" },
  { key: "uk", label: "Українська" },
]
</script>

<template>
  <USelectMenu
    :model-value="lang as LangKey"
    :items="langs"
    value-key="key"
    label-key="label"
    :search-input="false"
    :portal="false"
    variant="none"
    :content="{ sideOffset: 4 }"
    class="w-full bg-side-sel hover:border-accent text-side-fg border border-side-border text-[13px] cursor-pointer outline-none focus:border-accent rounded-none"
    :ui="{
      leading: 'ps-2.5',
      trailing: 'pe-2.5',
      value: 'flex-1 text-left text-side-fg',
      content: 'z-[1100] bg-side-sel border border-side-border ring-0 shadow-lg max-h-60 rounded-none',
      viewport: 'divide-y-0',
      group: 'p-0',
      item: 'flex items-center gap-2 px-2.5 py-1.5 text-[13px] cursor-pointer transition-colors duration-150 text-side-fg data-highlighted:bg-side-hover data-highlighted:text-white! before:hidden',
      itemTrailingIcon: 'size-3.5 text-accent',
    }"
    @update:model-value="(v: LangKey) => setLang(v)"
  >
    <template #leading="{ modelValue }">
      <!-- eslint-disable-next-line vue/no-v-html -- flags are static SVGs imported at build time. -->
      <span v-if="modelValue" class="flag-svg w-4 h-3 shrink-0" v-html="flags[modelValue as LangKey]" />
    </template>

    <template #trailing>
      <UIcon
        name="i-lucide-chevron-down"
        class="size-3.5 text-side-muted transition-transform duration-200 group-data-[state=open]:rotate-180"
      />
    </template>

    <template #item-leading="{ item }">
      <!-- eslint-disable-next-line vue/no-v-html -- flags are static SVGs imported at build time. -->
      <span class="flag-svg w-4 h-3 shrink-0" v-html="flags[item.key as LangKey]" />
    </template>
  </USelectMenu>
</template>
