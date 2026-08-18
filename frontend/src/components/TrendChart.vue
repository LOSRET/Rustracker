<script setup lang="ts">
import { computed } from "vue"
import VChart from "vue-echarts"
import { usePreferredDark } from "@vueuse/core"
import type { TrendsResponse, RangeKey } from "../types/api"
import { useI18n } from "../composables/useI18n"
import { baseChart, emptyChartOption, filterRange, lineSeries } from "../utils/chart"

const props = defineProps<{
  data: TrendsResponse | null
  error?: string | null
}>()
const range = defineModel<RangeKey>("range", { required: true })

const { t, d } = useI18n()
const isDark = usePreferredDark()

const option = computed(() => {
  const history = filterRange(props.data?.history ?? [], range.value)
  if (!history.length) {
    return emptyChartOption(props.error ? t("top100_error") : t("top100_empty"))
  }
  const dark = isDark.value
  const labels = history.map((item) => d(item.timestamp * 1000, "chart"))

  return {
    title: { text: "" },
    color: dark ? ["#3b82f6", "#94a3b8", "#22c55e", "#f59e0b"] : ["#2563eb", "#475569", "#15803d", "#b45309"],
    ...baseChart(dark, [t("torrents"), t("sort_peers"), t("sort_seeders"), t("sort_leechers")], labels),
    series: [
      lineSeries(
        t("torrents"),
        history.map((i) => i.torrents),
      ),
      lineSeries(
        t("sort_peers"),
        history.map((i) => i.peers),
      ),
      lineSeries(
        t("sort_seeders"),
        history.map((i) => i.seeders),
      ),
      lineSeries(
        t("sort_leechers"),
        history.map((i) => i.leechers),
      ),
    ],
  }
})

const rangeItems = computed(() =>
  (["24h", "3d", "7d"] as RangeKey[]).map((r) => ({ label: t(`range_${r}`), value: r })),
)
</script>

<template>
  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-baseline justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div>
        <h2 class="m-0 text-base leading-relaxed font-bold">{{ t("chart_title") }}</h2>
        <span class="text-muted text-xs">{{ t("chart_note") }}</span>
      </div>
      <URadioGroup
        v-model="range"
        :items="rangeItems"
        variant="table"
        orientation="horizontal"
        indicator="hidden"
        color="neutral"
        :ui="{
          fieldset: () => 'flex shrink-0 gap-0 -space-x-px',
          item: () =>
            'border text-muted px-3 text-xs cursor-pointer min-h-7 transition-colors flex items-center justify-center bg-panel border-line hover:bg-hover-soft has-data-[state=checked]:bg-accent has-data-[state=checked]:border-accent has-data-[state=checked]:text-white has-data-[state=checked]:z-[1] first-of-type:rounded-s last-of-type:rounded-e',
          container: () => 'contents',
          wrapper: () => 'contents',
          label: () => 'text-inherit font-normal',
        }"
      />
    </div>
    <div class="w-full h-[440px] max-[900px]:h-[330px] max-[560px]:h-[275px]">
      <v-chart :option="option" :init-options="{ renderer: 'svg' }" autoresize />
    </div>
  </section>
</template>
