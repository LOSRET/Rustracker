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
      'fixed md:sticky top-0 left-0 h-screen w-64 z-40 shrink-0 p-6 transition-transform',
      'bg-slate-900 text-slate-100 border-r border-slate-800',
      open ? 'translate-x-0' : '-translate-x-full md:translate-x-0',
    ]"
  >
    <div class="flex items-center gap-2 mb-6">
      <span class="inline-flex items-center justify-center w-8 h-8 rounded bg-blue-600 font-display font-bold">B</span>
      <span class="font-display">BitTorrent Tracker</span>
    </div>

    <div class="mb-4">
      <select
        v-model="lang"
        @change="onLangChange"
        class="w-full bg-slate-800 text-sm rounded px-3 py-2 border border-slate-700"
      >
        <option value="zh">中文</option>
        <option value="en">English</option>
        <option value="ja">日本語</option>
        <option value="ru">Русский</option>
        <option value="de">Deutsch</option>
        <option value="uk">Українська</option>
      </select>
    </div>

    <div class="text-xs uppercase tracking-wide text-slate-500 mb-2">{{ t.monitoring }}</div>
    <nav class="flex flex-col gap-1">
      <button
        :class="['text-left px-3 py-2 rounded transition-colors', page === 'dashboard' ? 'bg-slate-700' : 'hover:bg-slate-800']"
        @click="emit('switch', 'dashboard')"
      >
        {{ t.overview }}
      </button>
      <button
        :class="['text-left px-3 py-2 rounded transition-colors', page === 'top100' ? 'bg-slate-700' : 'hover:bg-slate-800']"
        @click="emit('switch', 'top100')"
      >
        {{ t.top100_link }}
      </button>
    </nav>

    <p class="mt-6 text-xs text-slate-500 leading-relaxed">{{ t.side_note }}</p>
  </aside>
</template>
