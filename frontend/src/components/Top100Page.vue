<script setup lang="ts">
import { computed, h, onMounted } from "vue"
import { FlexRender, getCoreRowModel, useVueTable, type ColumnDef } from "@tanstack/vue-table"
import type { SortKey, Top100Entry } from "../types/api"
import { useTop100 } from "../composables/useTop100"
import { useI18n } from "../composables/useI18n"

const { t, number, d } = useI18n()
const { data, loading, error, sort, lastUpdated, load } = useTop100()

onMounted(load)

function setSort(s: SortKey) {
  sort.value = s
}

const rows = computed(() => {
  if (!data.value) return []
  return data.value[sort.value]
})

const statusText = computed(() => {
  if (loading.value) return t("top100_loading")
  if (error.value) return t("top100_error")
  if (lastUpdated.value) return `${t("last_update")} ${d(lastUpdated.value, "time")}`
  return ""
})

const sortOptions: SortKey[] = ["peers", "seeders", "leechers", "downloaded"]
const sortLabel: Record<SortKey, string> = {
  peers: "sort_peers",
  seeders: "sort_seeders",
  leechers: "sort_leechers",
  downloaded: "sort_downloaded",
}

const sortItems = computed(() => sortOptions.map((s) => ({ label: t(sortLabel[s]), value: s })))

interface ColumnMeta {
  class?: {
    th?: string
    td?: string
  }
}

const columns = computed<ColumnDef<Top100Entry>[]>(() => [
  {
    id: "rank",
    header: "#",
    cell: ({ row }) => row.index + 1,
    meta: { class: { th: "w-12 text-center", td: "w-12 text-center text-muted font-semibold" } } as ColumnMeta,
  },
  {
    accessorKey: "info_hash",
    header: t("col_hash"),
    meta: { class: { td: "font-mono text-xs break-all" } } as ColumnMeta,
    cell: ({ row }) =>
      h("code", { class: "bg-code-bg px-1.5 py-0.5 rounded-sm text-xs" }, String(row.getValue("info_hash"))),
  },
  {
    accessorKey: "peers",
    header: t("sort_peers"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums" } } as ColumnMeta,
    cell: ({ row }) => number(row.getValue("peers") as number),
  },
  {
    accessorKey: "seeders",
    header: t("sort_seeders"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums text-good" } } as ColumnMeta,
    cell: ({ row }) => number(row.getValue("seeders") as number),
  },
  {
    accessorKey: "leechers",
    header: t("sort_leechers"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums text-warn" } } as ColumnMeta,
    cell: ({ row }) => number(row.getValue("leechers") as number),
  },
  {
    accessorKey: "downloaded",
    header: t("sort_downloaded"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums" } } as ColumnMeta,
    cell: ({ row }) => number(row.getValue("downloaded") as number),
  },
])

const tableData = computed(() => (loading.value || error.value ? [] : rows.value))

const table = useVueTable({
  get data() {
    return tableData.value
  },
  get columns() {
    return columns.value
  },
  getCoreRowModel: getCoreRowModel(),
})
</script>

<template>
  <section class="flex justify-between items-start gap-5 mb-6 max-[900px]:flex-col max-[900px]:items-stretch">
    <div>
      <h1 class="m-0 mb-1.5 text-[28px] leading-tight max-[560px]:text-[24px] font-bold">{{ t("top100_title") }}</h1>
      <p class="m-0 text-muted text-sm leading-relaxed">{{ t("top100_subtitle") }}</p>
    </div>
  </section>

  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-center justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div class="flex shrink-0 gap-0 -space-x-px" role="radiogroup">
        <label
          v-for="(item, idx) in sortItems"
          :key="item.value"
          :class="[
            'border text-muted px-4 text-[13px] cursor-pointer min-h-8 transition-colors flex items-center justify-center bg-panel border-line hover:bg-hover-soft',
            idx === 0 ? 'rounded-s' : '',
            idx === sortItems.length - 1 ? 'rounded-e' : '',
            sort === item.value ? 'bg-accent border-accent text-white z-[1]' : '',
          ]"
        >
          <input
            type="radio"
            class="sr-only"
            name="top100-sort"
            :value="item.value"
            :checked="sort === item.value"
            @change="setSort(item.value as SortKey)"
          />
          <span class="text-inherit font-normal">{{ item.label }}</span>
        </label>
      </div>
      <div class="flex items-center gap-3 max-[900px]:justify-between">
        <span class="text-muted text-xs whitespace-nowrap">{{ statusText }}</span>
        <button
          :disabled="loading"
          :class="[
            'border border-line bg-panel text-ink px-4 text-[13px] cursor-pointer min-h-8 rounded hover:bg-hover-soft',
            loading ? 'opacity-50 cursor-not-allowed' : '',
          ]"
          @click="load"
        >
          {{ t("refresh") }}
        </button>
      </div>
    </div>

    <div class="overflow-x-auto">
      <table class="min-w-full border-collapse">
        <thead>
          <tr>
            <th
              v-for="header in table.getHeaderGroups()[0]?.headers ?? []"
              :key="header.id"
              :class="[
                'p-2.5 bg-soft text-muted text-xs uppercase border-b-2 border-line whitespace-nowrap',
                (header.column.columnDef.meta as ColumnMeta)?.class?.th ?? '',
              ]"
            >
              <FlexRender
                v-if="!header.isPlaceholder"
                :render="header.column.columnDef.header"
                :props="header.getContext()"
              />
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in table.getRowModel().rows" :key="row.id" class="hover:bg-row-hover">
            <td
              v-for="cell in row.getVisibleCells()"
              :key="cell.id"
              :class="[
                'p-2 px-3 border-b border-td-border text-[13px] text-ink whitespace-normal',
                (cell.column.columnDef.meta as ColumnMeta)?.class?.td ?? '',
              ]"
            >
              <FlexRender :render="cell.column.columnDef.cell" :props="cell.getContext()" />
            </td>
          </tr>
          <tr v-if="loading">
            <td :colspan="columns.length" class="p-8 text-center text-[13px] text-muted">
              {{ t("top100_loading") }}
            </td>
          </tr>
          <tr v-else-if="error">
            <td :colspan="columns.length" class="p-8 text-center text-[13px] text-bad">
              {{ t("top100_error") }}
            </td>
          </tr>
          <tr v-else-if="tableData.length === 0">
            <td :colspan="columns.length" class="p-8 text-center text-[13px] text-muted">
              {{ t("top100_empty") }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>
