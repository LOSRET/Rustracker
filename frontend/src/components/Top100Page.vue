<script setup lang="ts">
import { computed } from "vue";
import type { SortKey } from "../types/api";
import { useTop100 } from "../composables/useTop100";
import { useI18n } from "../composables/useI18n";

const { t, number } = useI18n();
const { data, loading, error, sort, load } = useTop100();

const rows = computed(() => {
  if (!data.value) return [];
  return data.value[sort.value] ?? [];
});

const sortOptions: SortKey[] = ["peers", "seeders", "leechers", "downloaded"];
const sortLabel: Record<SortKey, string> = {
  peers: "sort_peers",
  seeders: "sort_seeders",
  leechers: "sort_leechers",
  downloaded: "sort_downloaded",
};
</script>

<template>
  <section class="mb-6">
    <h1 class="text-2xl font-display font-bold mb-1">{{ t.top100_title }}</h1>
    <p class="text-sm text-slate-500">{{ t.top100_subtitle }}</p>

    <div class="flex flex-wrap items-center gap-3 mt-6 mb-4">
      <div class="flex gap-1">
        <button
          v-for="s in sortOptions"
          :key="s"
          :class="['px-3 py-1 text-sm rounded transition-colors', sort === s ? 'bg-blue-600 text-white' : 'bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700']"
          @click="sort = s"
        >
          {{ t[sortLabel[s] as keyof typeof t] }}
        </button>
      </div>
      <button
        class="ml-auto px-3 py-1 text-sm rounded bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700"
        @click="load"
      >
        {{ t.refresh }}
      </button>
    </div>

    <div class="overflow-x-auto rounded-lg bg-white dark:bg-slate-900 shadow-sm">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-slate-200 dark:border-slate-800 text-left text-slate-500">
            <th class="p-3 w-12">#</th>
            <th class="p-3">{{ t.col_hash }}</th>
            <th class="p-3 text-right">Peers</th>
            <th class="p-3 text-right">Seeders</th>
            <th class="p-3 text-right">Leechers</th>
            <th class="p-3 text-right">Downloaded</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="loading">
            <td colspan="6" class="p-8 text-center text-slate-500">{{ t.top100_loading }}</td>
          </tr>
          <tr v-else-if="error">
            <td colspan="6" class="p-8 text-center text-red-500">{{ t.top100_error }}</td>
          </tr>
          <tr v-else-if="!rows.length">
            <td colspan="6" class="p-8 text-center text-slate-500">{{ t.top100_empty }}</td>
          </tr>
          <tr
            v-else
            v-for="(row, i) in rows"
            :key="row.info_hash"
            class="border-b border-slate-100 dark:border-slate-800/50 hover:bg-slate-50 dark:hover:bg-slate-800/30"
          >
            <td class="p-3 text-slate-400 tabular-nums">{{ i + 1 }}</td>
            <td class="p-3 font-mono text-xs break-all">{{ row.info_hash }}</td>
            <td class="p-3 text-right tabular-nums">{{ number(row.peers) }}</td>
            <td class="p-3 text-right tabular-nums">{{ number(row.seeders) }}</td>
            <td class="p-3 text-right tabular-nums">{{ number(row.leechers) }}</td>
            <td class="p-3 text-right tabular-nums">{{ number(row.downloaded) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>
