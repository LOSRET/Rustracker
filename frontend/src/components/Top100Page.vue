<script setup lang="ts">
import { computed, h, onMounted } from "vue";
import type { TableColumn } from "@nuxt/ui";
import type { SortKey, Top100Entry } from "../types/api";
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

const columns = computed<TableColumn<Top100Entry>[]>(() => [
  {
    id: "rank",
    header: "#",
    cell: ({ row }) => row.index + 1,
    meta: { class: { th: "w-12 text-center", td: "w-12 text-center text-muted font-semibold" } },
  },
  {
    accessorKey: "info_hash",
    header: t("col_hash"),
    meta: { class: { td: "font-mono text-xs break-all" } },
    cell: ({ row }) => h("code", { class: "bg-code-bg px-1.5 py-0.5 rounded-sm text-xs" }, String(row.getValue("info_hash"))),
  },
  {
    accessorKey: "peers",
    header: t("sort_peers"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums" } },
    cell: ({ row }) => number(row.getValue("peers") as number),
  },
  {
    accessorKey: "seeders",
    header: t("sort_seeders"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums text-good" } },
    cell: ({ row }) => number(row.getValue("seeders") as number),
  },
  {
    accessorKey: "leechers",
    header: t("sort_leechers"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums text-warn" } },
    cell: ({ row }) => number(row.getValue("leechers") as number),
  },
  {
    accessorKey: "downloaded",
    header: t("sort_downloaded"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums" } },
    cell: ({ row }) => number(row.getValue("downloaded") as number),
  },
]);

const tableData = computed(() => (loading.value || error.value ? [] : rows.value));
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
      <UTable
        :data="tableData"
        :columns="columns"
        :loading="loading"
        :ui="{
          root: 'overflow-visible',
          base: 'min-w-full',
          tbody: 'divide-y-0',
          tr: 'hover:bg-row-hover',
          th: 'p-2.5 bg-soft text-muted text-xs uppercase border-b-2 border-line whitespace-nowrap',
          td: 'p-2 px-3 border-b border-td-border text-[13px] text-ink whitespace-normal',
          empty: 'p-8 text-center text-[13px]',
          loading: 'p-8 text-center text-[13px]',
        }"
      >
        <template #loading>
          <span class="text-muted">{{ t('top100_loading') }}</span>
        </template>
        <template #empty>
          <span v-if="error" class="text-bad">{{ t('top100_error') }}</span>
          <span v-else class="text-muted">{{ t('top100_empty') }}</span>
        </template>
      </UTable>
    </div>
  </section>
</template>
