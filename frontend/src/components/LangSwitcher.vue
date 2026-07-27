<script setup lang="ts">
import {
  SelectRoot,
  SelectTrigger,
  SelectValue,
  SelectIcon,
  SelectPortal,
  SelectContent,
  SelectViewport,
  SelectItem,
  SelectItemText,
  SelectItemIndicator,
} from "reka-ui"
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
  <SelectRoot :model-value="lang as LangKey" @update:model-value="(v: LangKey) => setLang(v)">
    <SelectTrigger
      class="group w-full flex items-center bg-side-sel hover:border-accent text-side-fg border border-side-border text-[13px] cursor-pointer outline-none focus:border-accent rounded-none min-h-[34px]"
    >
      <span class="flag-svg w-4 h-3 shrink-0 ms-2.5">
        <!-- eslint-disable-next-line vue/no-v-html -- flags are static SVGs imported at build time. -->
        <span v-if="lang" v-html="flags[lang as LangKey]" />
      </span>
      <SelectValue class="flex-1 text-left text-side-fg ps-2">
        {{ langs.find((l) => l.key === lang)?.label }}
      </SelectValue>
      <SelectIcon class="me-2.5 text-side-muted transition-transform duration-200 group-data-[state=open]:rotate-180">
        <svg class="w-3.5 h-3.5" viewBox="0 0 20 20" fill="currentColor">
          <path
            fill-rule="evenodd"
            d="M5.23 7.21a.75.75 0 011.06.02L10 11.06l3.71-3.83a.75.75 0 111.08 1.04l-4.25 4.39a.75.75 0 01-1.08 0L5.21 8.27a.75.75 0 01.02-1.06z"
            clip-rule="evenodd"
          />
        </svg>
      </SelectIcon>
    </SelectTrigger>

    <SelectPortal>
      <SelectContent
        :side-offset="4"
        position="popper"
        class="z-[1100] bg-side-sel border border-side-border ring-0 shadow-lg max-h-60 rounded-none overflow-hidden"
      >
        <SelectViewport class="p-0">
          <SelectItem
            v-for="item in langs"
            :key="item.key"
            :value="item.key"
            class="flex items-center gap-2 px-2.5 py-1.5 text-[13px] cursor-pointer transition-colors duration-150 text-side-fg data-highlighted:bg-side-hover data-highlighted:!text-white outline-none"
          >
            <span class="flag-svg w-4 h-3 shrink-0">
              <!-- eslint-disable-next-line vue/no-v-html -- flags are static SVGs imported at build time. -->
              <span v-html="flags[item.key]" />
            </span>
            <SelectItemText>{{ item.label }}</SelectItemText>
            <SelectItemIndicator class="size-3.5 text-accent">
              <svg viewBox="0 0 20 20" fill="currentColor">
                <path
                  fill-rule="evenodd"
                  d="M16.704 5.29a1 1 0 010 1.42l-7.5 7.5a1 1 0 01-1.42 0l-3.5-3.5a1 1 0 011.42-1.42L8.5 12.09l6.79-6.8a1 1 0 011.414 0z"
                  clip-rule="evenodd"
                />
              </svg>
            </SelectItemIndicator>
          </SelectItem>
        </SelectViewport>
      </SelectContent>
    </SelectPortal>
  </SelectRoot>
</template>
