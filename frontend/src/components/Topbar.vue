<script setup lang="ts">
import type { StatsResponse } from "../types/api"
import { useI18n } from "../composables/useI18n"
import CopyButton from "../components/CopyButton.vue"
import PageHeader from "../components/PageHeader.vue"

const { stats, error, lastUpdated } = defineProps<{
  stats: StatsResponse | null
  error: string | null
  lastUpdated: number | null
}>()

const { t, number, d } = useI18n()

const trackerUrl = `${window.location.origin}/announce`

function lastUpdateText() {
  const ts = lastUpdated
  if (ts == null) return t("loading")
  return `${t("last_update")} ${d(ts, "time")}`
}
</script>

<template>
  <PageHeader :title="t('title')" :subtitle="t('subtitle')">
    <p :class="['text-xs mt-1.5', error ? 'text-bad' : 'text-muted']">
      {{ error ? t("error") : lastUpdateText() }}
    </p>
    <div class="flex items-center flex-wrap gap-1 mb-5">
      <span class="text-muted text-sm font-normal whitespace-nowrap">{{ t("tracker_addr_label") }}</span>
      <CopyButton
        :value="trackerUrl"
        :copy-label="t('copy_tracker')"
        class="tracker-addr text-left text-base font-bold break-all border-b border-dashed border-line"
      />
    </div>
    <template #actions>
      <div class="text-right shrink-0 bg-soft px-3.5 py-2 rounded max-[900px]:text-left">
        <div
          class="flex items-center justify-end gap-1.5 text-muted text-[11px] uppercase mb-0.5 max-[900px]:justify-start"
        >
          <i class="w-[7px] h-[7px] bg-good rounded-full shrink-0 animate-rps-pulse"></i>
          Tracker RPS
        </div>
        <strong class="text-[22px] leading-none text-ink max-[900px]:text-xl">{{
          number(stats?.tracker_rps ?? 0)
        }}</strong>
      </div>
    </template>
  </PageHeader>
</template>
