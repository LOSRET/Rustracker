<script setup lang="ts">
import type { PageKey } from "../types/api"
import { useI18n } from "../composables/useI18n"
import LangSwitcher from "./LangSwitcher.vue"

const props = defineProps<{ page: PageKey; error?: string | null }>()
const emit = defineEmits<{ switch: [page: PageKey] }>()

const { t } = useI18n()

function scrollToDisclaimer() {
  document.getElementById("disclaimer")?.scrollIntoView({ behavior: "smooth" })
  emit("switch", props.page)
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
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24">
          <path
            fill="currentColor"
            d="m11.9 22l4.55-12h2.1l4.55 12H21l-1.075-3.05h-4.85L14 22zM4 19l-1.4-1.4l5.05-5.05q-.875-.875-1.588-2T4.75 8h2.1q.5.975 1 1.7t1.2 1.45q.825-.825 1.713-2.313T12.1 6H1V4h7V2h2v2h7v2h-2.9q-.525 1.8-1.575 3.7t-2.075 2.9l2.4 2.45l-.75 2.05l-3.05-3.125zm11.7-1.8h3.6l-1.8-5.1z"
          />
        </svg>
      </span>
      <LangSwitcher class="flex-1" />
    </div>

    <div class="text-side-muted text-xs uppercase mb-2.5">{{ t("monitoring") }}</div>
    <nav class="flex flex-col">
      <button
        :class="[
          'flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 max-[900px]:cursor-pointer',
          page === 'dashboard' ? 'bg-side-active border-accent' : 'border-transparent hover:bg-side-hover',
        ]"
        @click="emit('switch', 'dashboard')"
      >
        <span>{{ t("overview") }}</span>
        <span>{{ error ? t("error") : t("running") }}</span>
      </button>
      <button
        :class="[
          'flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 max-[900px]:cursor-pointer',
          page === 'top100' ? 'bg-side-active border-accent' : 'border-transparent hover:bg-side-hover',
        ]"
        @click="emit('switch', 'top100')"
      >
        <span>🏆 {{ t("top100_link") }}</span>
        <span>→</span>
      </button>
      <button
        :class="[
          'flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 max-[900px]:cursor-pointer',
          page === 'clients' ? 'bg-side-active border-accent' : 'border-transparent hover:bg-side-hover',
        ]"
        @click="emit('switch', 'clients')"
      >
        <span>📊 {{ t("clients_link") }}</span>
        <span>→</span>
      </button>
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
