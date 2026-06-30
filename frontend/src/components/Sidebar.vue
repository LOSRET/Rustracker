<script setup lang="ts">
import type { PageKey, LangKey } from "../types/api";
import { useI18n } from "../composables/useI18n";

defineProps<{ page: PageKey; open: boolean }>();
const emit = defineEmits<{ switch: [page: PageKey] }>();

const { lang, t, setLang } = useI18n();

function onLangChange(e: Event) {
  setLang((e.target as HTMLSelectElement).value as LangKey);
}
</script>

<template>
  <aside
    :class="[
      'bg-side text-[#f8fafc] sticky top-0 h-screen overflow-y-auto p-6 max-[900px]:fixed max-[900px]:top-0 max-[900px]:left-[-280px] max-[900px]:w-[260px] max-[900px]:h-screen max-[900px]:p-5 max-[900px]:z-[1000] max-[900px]:transition-[left] max-[900px]:duration-200',
      open ? 'max-[900px]:left-0' : '',
    ]"
  >
    <div class="flex items-center gap-2.5 text-xl font-bold mb-7">
      <span class="w-7 h-7 bg-accent grid place-items-center text-white text-[15px] font-extrabold font-display">B</span>
      <span>BitTorrent Tracker</span>
    </div>

    <div class="mb-5 flex items-center gap-2">
      <span class="flex items-center text-[#9ca3af] shrink-0">
        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24"><path fill="currentColor" d="m11.9 22l4.55-12h2.1l4.55 12H21l-1.075-3.05h-4.85L14 22zM4 19l-1.4-1.4l5.05-5.05q-.875-.875-1.588-2T4.75 8h2.1q.5.975 1 1.7t1.2 1.45q.825-.825 1.713-2.313T12.1 6H1V4h7V2h2v2h7v2h-2.9q-.525 1.8-1.575 3.7t-2.075 2.9l2.4 2.45l-.75 2.05l-3.05-3.125zm11.7-1.8h3.6l-1.8-5.1z"/></svg>
      </span>
      <select
        v-model="lang"
        @change="onLangChange"
        class="w-full bg-side-sel text-[#f8fafc] border border-side-border px-2.5 py-1.5 text-[13px] cursor-pointer"
      >
        <option value="zh">中文</option>
        <option value="en">English</option>
        <option value="ja">日本語</option>
        <option value="ru">Русский</option>
        <option value="de">Deutsch</option>
        <option value="uk">Українська</option>
      </select>
    </div>

    <div class="text-[#9ca3af] text-xs uppercase mb-2.5">{{ t.monitoring }}</div>
    <nav class="flex flex-col">
      <button
        :class="[
          'flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 max-[900px]:cursor-pointer',
          page === 'dashboard'
            ? 'bg-side-active border-accent'
            : 'border-transparent hover:bg-side-hover',
        ]"
        @click="emit('switch', 'dashboard')"
      >
        <span>{{ t.overview }}</span>
        <span>{{ page === 'dashboard' ? t.running : '' }}</span>
      </button>
      <button
        :class="[
          'flex items-center justify-between px-3 py-2.5 text-white text-sm border-l-4 max-[900px]:cursor-pointer',
          page === 'top100'
            ? 'bg-side-active border-accent'
            : 'border-transparent hover:bg-side-hover',
        ]"
        @click="emit('switch', 'top100')"
      >
        <span>🏆 {{ t.top100_link }}</span>
        <span>→</span>
      </button>
    </nav>

    <p class="mt-6 text-[#cbd5e1] text-[13px] leading-relaxed">{{ t.side_note }}</p>
  </aside>
</template>
