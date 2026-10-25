<script setup lang="ts" generic="T">
import { computed, useSlots } from "vue"
import type { TableColumn } from "@nuxt/ui"
import { useI18n } from "../composables/useI18n"

const props = defineProps<{
  data: T[]
  columns: TableColumn<T>[]
  loading?: boolean
  error?: string | null
  lastUpdated?: number | null
}>()

const emit = defineEmits<{ refresh: [] }>()

const { t, d } = useI18n()
const slots = useSlots()

const statusText = computed(() => {
  if (props.loading) return t("table_loading")
  if (props.error) return t("table_error")
  if (props.lastUpdated) return `${t("last_update")} ${d(props.lastUpdated, "time")}`
  return ""
})

const displayData = computed(() => (props.loading || props.error ? [] : props.data))
</script>

<template>
  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-center justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <slot name="controls" />
      <div class="flex items-center gap-3 max-[900px]:justify-between" :class="slots.controls ? '' : 'ml-auto'">
        <span class="text-muted text-xs whitespace-nowrap">{{ statusText }}</span>
        <UButton
          :disabled="loading"
          variant="none"
          :class="[
            'border border-line bg-panel text-ink px-4 text-[13px] cursor-pointer min-h-8 rounded hover:bg-hover-soft',
            loading ? 'opacity-50 cursor-not-allowed' : '',
          ]"
          @click="emit('refresh')"
        >
          {{ t("refresh") }}
        </UButton>
      </div>
    </div>

    <div class="overflow-x-auto">
      <UTable
        :data="displayData"
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
          <span class="text-muted">{{ t("table_loading") }}</span>
        </template>
        <template #empty>
          <span v-if="error" class="text-bad">{{ t("table_error") }}</span>
          <span v-else class="text-muted">{{ t("table_empty") }}</span>
        </template>
      </UTable>
    </div>
  </section>
</template>
