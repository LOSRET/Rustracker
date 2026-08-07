<script setup lang="ts">
import { computed, onMounted } from "vue"
import type { TableColumn } from "@nuxt/ui"
import type { ClientListEntry } from "../types/api"
import { useClientsList } from "../composables/useClientsList"
import { useI18n } from "../composables/useI18n"

const { t, number, d, n } = useI18n()
const { data, loading, error, lastUpdated, load } = useClientsList()

onMounted(load)

const totalPeers = computed(() => (data.value ? data.value.clients.reduce((s, c) => s + c.peers, 0) : 0))

const rows = computed(() => data.value?.clients ?? [])

const statusText = computed(() => {
  if (loading.value) return t("top100_loading")
  if (error.value) return t("top100_error")
  if (lastUpdated.value) return `${t("last_update")} ${d(lastUpdated.value, "time")}`
  return ""
})

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

const tableData = computed(() => (loading.value || error.value ? [] : rows.value))
</script>

<template>
  <section class="flex justify-between items-start gap-5 mb-6 max-[900px]:flex-col max-[900px]:items-stretch">
    <div>
      <h1 class="m-0 mb-1.5 text-[28px] leading-tight max-[560px]:text-[24px] font-bold">{{ t("clients_title") }}</h1>
      <p class="m-0 text-muted text-sm leading-relaxed">{{ t("clients_subtitle") }}</p>
    </div>
  </section>

  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-center justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div class="flex items-center justify-end gap-3 w-full">
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
          {{ t("refresh") }}
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
          <span class="text-muted">{{ t("top100_loading") }}</span>
        </template>
        <template #empty>
          <span v-if="error" class="text-bad">{{ t("top100_error") }}</span>
          <span v-else class="text-muted">{{ t("top100_empty") }}</span>
        </template>
      </UTable>
    </div>
  </section>
</template>
