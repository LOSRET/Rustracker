<script setup lang="ts">
import type { PageKey } from "../types/api"
import { useI18n } from "../composables/useI18n"
import LangSwitcher from "./LangSwitcher.vue"

defineProps<{ page: PageKey; error?: string | null }>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()

function scrollToDisclaimer() {
  document.getElementById("disclaimer")?.scrollIntoView({ behavior: "smooth" })
  emit("close")
}
</script>

<template>
  <div>
    <div class="flex items-center gap-2.5 text-xl font-bold mb-7">
      <span
        class="w-7 h-7 leading-none bg-accent grid place-items-center text-white text-[15px] font-bold font-display shrink-0"
        >B</span
      >
      <span class="whitespace-nowrap">BitTorrent Tracker</span>
    </div>

    <div class="mb-5 flex items-center gap-2">
      <span class="flex items-center text-side-muted shrink-0">
        <UIcon name="i-lucide-languages" class="size-5" />
      </span>
      <LangSwitcher class="flex-1" />
    </div>

    <div class="text-side-muted text-xs uppercase mb-2.5">{{ t("monitoring") }}</div>
    <nav class="flex flex-col">
      <RouterLink
        :to="{ name: 'dashboard' }"
        :class="[
          'flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 max-[900px]:cursor-pointer no-underline',
          page === 'dashboard' ? 'bg-side-active border-accent' : 'border-transparent hover:bg-side-hover',
        ]"
      >
        <span>{{ t("overview") }}</span>
        <span>{{ error ? t("error") : t("running") }}</span>
      </RouterLink>
      <RouterLink
        :to="{ name: 'top100' }"
        :class="[
          'flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 max-[900px]:cursor-pointer no-underline',
          page === 'top100' ? 'bg-side-active border-accent' : 'border-transparent hover:bg-side-hover',
        ]"
      >
        <span>{{ t("top100_link") }}</span>
        <span>→</span>
      </RouterLink>
      <RouterLink
        :to="{ name: 'clients' }"
        :class="[
          'flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 max-[900px]:cursor-pointer no-underline',
          page === 'clients' ? 'bg-side-active border-accent' : 'border-transparent hover:bg-side-hover',
        ]"
      >
        <span>{{ t("clients_link") }}</span>
        <span>→</span>
      </RouterLink>
      <a
        class="flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 border-transparent hover:bg-side-hover mt-2.5 no-underline cursor-pointer"
        href="#disclaimer"
        @click.prevent="scrollToDisclaimer"
      >
        <span>{{ t("disc_link") }}</span>
        <span>{{ t("view") }}</span>
      </a>
    </nav>

    <p class="mt-6 text-side-note text-[13px] leading-relaxed">{{ t("side_note") }}</p>
  </div>
</template>
