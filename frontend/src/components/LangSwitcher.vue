<script setup lang="ts">
import { Listbox, ListboxButton, ListboxOptions, ListboxOption, TransitionRoot } from "@headlessui/vue";
import { CN, GB, JP, RU, DE, UA } from "country-flag-icons/string/3x2";
import type { LangKey } from "../types/api";
import { useI18n } from "../composables/useI18n";

const { lang, setLang } = useI18n();

const flags: Record<LangKey, string> = { zh: CN, en: GB, ja: JP, ru: RU, de: DE, uk: UA };

const langs: { key: LangKey; label: string }[] = [
  { key: "zh", label: "中文" },
  { key: "en", label: "English" },
  { key: "ja", label: "日本語" },
  { key: "ru", label: "Русский" },
  { key: "de", label: "Deutsch" },
  { key: "uk", label: "Українська" },
];
</script>

<template>
  <Listbox :modelValue="lang" @update:modelValue="setLang">
    <div class="relative">
      <ListboxButton
        v-slot="{ open }"
        class="w-full flex items-center gap-2 bg-side-sel text-[#f8fafc] border border-side-border px-2.5 py-1.5 text-[13px] cursor-pointer outline-none focus:border-accent"
      >
        <span class="flag-svg w-4 h-3 shrink-0" v-html="flags[lang as LangKey]" />
        <span class="flex-1 text-left">{{ langs.find((l) => l.key === lang)?.label }}</span>
        <svg :class="['w-3.5 h-3.5 text-[#9ca3af] transition-transform duration-200', open && 'rotate-180']" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.06l3.71-3.83a.75.75 0 111.08 1.04l-4.25 4.39a.75.75 0 01-1.08 0L5.21 8.27a.75.75 0 01.02-1.06z" clip-rule="evenodd" /></svg>
      </ListboxButton>

      <TransitionRoot
        enter="transition ease-out duration-150"
        enterFrom="opacity-0 scale-95"
        enterTo="opacity-100 scale-100"
        leave="transition ease-in duration-100"
        leaveFrom="opacity-100 scale-100"
        leaveTo="opacity-0 scale-95"
      >
        <ListboxOptions
          class="absolute z-[1100] mt-1 w-full origin-top bg-side-sel border border-side-border shadow-lg max-h-60 overflow-auto outline-none"
        >
          <ListboxOption
            v-for="l in langs"
            :key="l.key"
            :value="l.key"
            v-slot="{ active, selected }"
          >
            <div
              :class="[
                'flex items-center gap-2 px-2.5 py-1.5 text-[13px] cursor-pointer transition-colors duration-150',
                active ? 'bg-side-active text-white' : 'text-[#f8fafc]',
              ]"
            >
              <span class="flag-svg w-4 h-3 shrink-0" v-html="flags[l.key]" />
              <span class="flex-1">{{ l.label }}</span>
              <svg v-if="selected" class="w-3.5 h-3.5 text-accent" viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M16.7 5.3a1 1 0 010 1.4l-7.5 7.5a1 1 0 01-1.4 0l-3.5-3.5a1 1 0 111.4-1.4l2.8 2.79 6.8-6.79a1 1 0 011.4 0z" clip-rule="evenodd" /></svg>
            </div>
          </ListboxOption>
        </ListboxOptions>
      </TransitionRoot>
    </div>
  </Listbox>
</template>
