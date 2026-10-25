<script setup lang="ts">
import { computed, onActivated } from "vue"
import type { TableColumn } from "@nuxt/ui"
import type { ClientListEntry } from "../types/api"
import { useClientsList } from "../composables/useClientsList"
import { useI18n } from "../composables/useI18n"
import PageHeader from "../components/PageHeader.vue"
import TablePanel from "../components/TablePanel.vue"

const { t, number, n } = useI18n()
const { data, loading, error, lastUpdated, load } = useClientsList()

onActivated(load)

const totalPeers = computed(() => (data.value ? data.value.clients.reduce((s, c) => s + c.peers, 0) : 0))

const rows = computed(() => data.value?.clients ?? [])

function share(peers: number): string {
  if (!totalPeers.value) return "—"
  return n(peers / totalPeers.value, "percent")
}

const columns = computed<TableColumn<ClientListEntry>[]>(() => [
  {
    id: "rank",
    header: "#",
    cell: ({ row }) => row.index + 1,
    meta: { class: { th: "w-12 text-center", td: "w-12 text-center text-muted font-semibold" } },
  },
  {
    accessorKey: "name",
    header: t("clients_col_name"),
    meta: { class: { td: "font-medium" } },
  },
  {
    accessorKey: "peers",
    header: t("sort_peers"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums" } },
    cell: ({ row }) => number(row.getValue("peers") as number),
  },
  {
    id: "share",
    header: t("clients_col_share"),
    meta: { class: { th: "text-right", td: "text-right whitespace-nowrap tabular-nums text-muted" } },
    cell: ({ row }) => share(row.original.peers),
  },
])
</script>

<template>
  <PageHeader :title="t('clients_title')" :subtitle="t('clients_subtitle')" />
  <TablePanel
    :data="rows"
    :columns="columns"
    :loading="loading"
    :error="error"
    :last-updated="lastUpdated"
    @refresh="load"
  />
</template>
