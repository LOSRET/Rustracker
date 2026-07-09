<script setup lang="ts">
import { computed, onMounted } from "vue";
import type { SortKey } from "../types/api";
import { useTop100 } from "../composables/useTop100";
import { useI18n } from "../composables/useI18n";

const { t, number, d } = useI18n();
const { data, loading, error, sort, lastUpdated, load } = useTop100();

onMounted(load);

const rows = computed(() => {
  if (!data.value) return [];
  return data.value[sort.value];
});

const statusText = computed(() => {
  if (loading.value) return t("top100_loading");
  if (error.value) return t("top100_error");
  if (lastUpdated.value)
    return `${t("last_update")} ${d(lastUpdated.value, "time")}`;
  return "";
});

const sortOptions: SortKey[] = ["peers", "seeders", "leechers", "downloaded"];
const sortLabel: Record<SortKey, string> = {
  peers: "sort_peers",
  seeders: "sort_seeders",
  leechers: "sort_leechers",
  downloaded: "sort_downloaded",
};

const sortItems = computed(() => sortOptions.map((s) => ({ label: t(sortLabel[s]), value: s })));
</script>

<template>
  <section class="flex justify-between items-start gap-5 mb-6 max-[900px]:flex-col max-[900px]:items-stretch">
    <div>
      <h1 class="m-0 mb-1.5 text-[28px] leading-tight max-[560px]:text-[24px] font-bold">{{ t('top100_title') }}</h1>
      <p class="m-0 text-muted text-sm leading-relaxed">{{ t('top100_subtitle') }}</p>
    </div>
  </section>

  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-center justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <URadioGroup
        v-model="sort"
        :items="sortItems"
        variant="table"
        orientation="horizontal"
        indicator="hidden"
        color="neutral"
        :ui="{
          fieldset: () => 'flex shrink-0 gap-0 -space-x-px',
          item: () => 'border text-muted px-4 text-[13px] cursor-pointer min-h-8 transition-colors flex items-center justify-center bg-panel border-line hover:bg-hover-soft has-data-[state=checked]:bg-accent has-data-[state=checked]:border-accent has-data-[state=checked]:text-white has-data-[state=checked]:z-[1] first-of-type:rounded-s last-of-type:rounded-e',
          container: () => 'contents',
          wrapper: () => 'contents',
          label: () => 'text-inherit font-normal',
        }"
      />
      <div class="flex items-center gap-3 max-[900px]:justify-between">
        <span class="text-muted text-xs whitespace-nowrap">{{ statusText }}</span>
        <UButton
          :disabled="loading"
          variant="none"
          :class="[
            'border border-line bg-panel text-ink px-4 text-[13px] cursor-pointer min-h-8 rounded hover:bg-hover-soft',
            loading ? 'opacity-50 cursor-not-allowed' : '',
          ]"
          @click="load"
        >
          {{ t('refresh') }}
        </UButton>
      </div>
    </div>

    <div class="overflow-x-auto">
      <table class="w-full border-collapse text-[13px]">
        <thead>
          <tr>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap w-12 text-center">#</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap">{{ t('col_hash') }}</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">{{ t('sort_peers') }}</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">{{ t('sort_seeders') }}</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">{{ t('sort_leechers') }}</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">{{ t('sort_downloaded') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="loading">
            <td colspan="6" class="p-8 text-center text-muted">{{ t('top100_loading') }}</td>
          </tr>
          <tr v-else-if="error">
            <td colspan="6" class="p-8 text-center text-bad">{{ t('top100_error') }}</td>
          </tr>
          <tr v-else-if="!rows.length">
            <td colspan="6" class="p-8 text-center text-muted">{{ t('top100_empty') }}</td>
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
