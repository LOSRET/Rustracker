<script setup lang="ts">
import { computed, h, onActivated } from "vue"
import type { TableColumn } from "@nuxt/ui"
import type { SortKey, Top100Entry } from "../types/api"
import { useTop100 } from "../composables/useTop100"
import { useI18n } from "../composables/useI18n"
import CopyButton from "../components/CopyButton.vue"
import PageHeader from "../components/PageHeader.vue"
import SegmentedControl from "../components/SegmentedControl.vue"
import TablePanel from "../components/TablePanel.vue"

const { t, number } = useI18n()
const { data, loading, error, sort, lastUpdated, load } = useTop100()

onActivated(load)

const rows = computed(() => {
  if (!data.value) return []
  return data.value[sort.value]
})

const sortOptions: SortKey[] = ["peers", "seeders", "leechers", "downloaded"]
const sortLabel: Record<SortKey, string> = {
  peers: "sort_peers",
  seeders: "sort_seeders",
  leechers: "sort_leechers",
  downloaded: "sort_downloaded",
}

const sortItems = computed(() => sortOptions.map((s) => ({ label: t(sortLabel[s]), value: s })))

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
    cell: ({ row }) => {
      const hash = String(row.getValue("info_hash"))
      return h(
        CopyButton,
        {
          value: hash,
          copyLabel: t("copy_hash"),
          class: "inline-block max-w-full bg-code-bg px-1.5 py-0.5 text-left font-mono text-xs break-all rounded-sm",
        },
        () => hash,
      )
    },
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
])
</script>

<template>
  <PageHeader :title="t('top100_title')" :subtitle="t('top100_subtitle')" />
  <TablePanel
    :data="rows"
    :columns="columns"
    :loading="loading"
    :error="error"
    :last-updated="lastUpdated"
    @refresh="load"
  >
    <template #controls>
      <SegmentedControl v-model="sort" :items="sortItems" />
    </template>
  </TablePanel>
</template>
