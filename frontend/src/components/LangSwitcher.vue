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
      item: 'flex items-center gap-2 px-2.5 py-1.5 text-[13px] cursor-pointer transition-colors duration-150 text-side-fg data-highlighted:bg-side-hover data-highlighted:!text-white before:hidden',
      itemTrailingIcon: 'size-3.5 text-accent',
    }"
    @update:model-value="(v: LangKey) => setLang(v)"
  >
    <template #leading="{ modelValue }">
      <span v-if="modelValue" class="flag-svg w-4 h-3 shrink-0" v-html="flags[modelValue as LangKey]" />
    </template>

    <template #trailing>
      <svg
        class="w-3.5 h-3.5 text-side-muted transition-transform duration-200 group-data-[state=open]:rotate-180"
        viewBox="0 0 20 20"
        fill="currentColor"
      >
        <path
          fill-rule="evenodd"
          d="M5.23 7.21a.75.75 0 011.06.02L10 11.06l3.71-3.83a.75.75 0 111.08 1.04l-4.25 4.39a.75.75 0 01-1.08 0L5.21 8.27a.75.75 0 01.02-1.06z"
          clip-rule="evenodd"
        />
      </svg>
    </template>

    <template #item-leading="{ item }">
      <span class="flag-svg w-4 h-3 shrink-0" v-html="flags[item.key as LangKey]" />
    </template>
  </USelectMenu>
</template>
