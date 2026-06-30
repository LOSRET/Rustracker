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
  <section class="flex justify-between items-start gap-5 mb-6 max-[900px]:flex-col max-[900px]:items-stretch">
    <div>
      <h1 class="m-0 mb-1.5 text-[28px] leading-tight">{{ t.top100_title }}</h1>
      <p class="m-0 text-muted text-sm leading-relaxed">{{ t.top100_subtitle }}</p>
    </div>
  </section>

  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-center justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div class="flex shrink-0">
        <button
          v-for="(s, i) in sortOptions"
          :key="s"
          :class="[
            'border border-line bg-panel text-muted px-4 text-[13px] cursor-pointer min-h-8 transition-colors',
            i === 0 ? 'rounded-l' : 'border-l-0',
            i === 3 ? 'rounded-r' : '',
            sort === s ? 'bg-accent border-accent text-white' : 'hover:bg-hover-soft',
          ]"
          @click="sort = s"
        >
          {{ t[sortLabel[s] as keyof typeof t] }}
        </button>
      </div>
      <div class="flex items-center gap-3 max-[900px]:justify-between">
        <span class="text-muted text-xs whitespace-nowrap"></span>
        <button
          class="border border-line bg-panel text-ink px-4 text-[13px] cursor-pointer min-h-8 rounded hover:bg-hover-soft"
          @click="load"
        >
          {{ t.refresh }}
        </button>
      </div>
    </div>

    <div class="overflow-x-auto">
      <table class="w-full border-collapse text-[13px]">
        <thead>
          <tr>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap w-12 text-center">#</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap">{{ t.col_hash }}</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">Peers</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">Seeders</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">Leechers</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">Downloaded</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="loading">
            <td colspan="6" class="p-8 text-center text-muted">{{ t.top100_loading }}</td>
          </tr>
          <tr v-else-if="error">
            <td colspan="6" class="p-8 text-center text-bad">{{ t.top100_error }}</td>
          </tr>
          <tr v-else-if="!rows.length">
            <td colspan="6" class="p-8 text-center text-muted">{{ t.top100_empty }}</td>
          </tr>
          <template v-else>
            <tr
              v-for="(row, i) in rows"
              :key="row.info_hash"
              class="hover:bg-row-hover"
            >
              <td class="p-2 px-3 border-b border-td-border text-center text-muted font-semibold w-12">{{ i + 1 }}</td>
              <td class="p-2 px-3 border-b border-td-border font-mono text-xs break-all">
                <code class="bg-code-bg px-1.5 py-0.5 rounded-sm text-xs">{{ row.info_hash }}</code>
              </td>
              <td class="p-2 px-3 border-b border-td-border text-right whitespace-nowrap tabular-nums">{{ number(row.peers) }}</td>
              <td class="p-2 px-3 border-b border-td-border text-right whitespace-nowrap tabular-nums text-good">{{ number(row.seeders) }}</td>
              <td class="p-2 px-3 border-b border-td-border text-right whitespace-nowrap tabular-nums text-warn">{{ number(row.leechers) }}</td>
              <td class="p-2 px-3 border-b border-td-border text-right whitespace-nowrap tabular-nums">{{ number(row.downloaded) }}</td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
  </section>
</template>
