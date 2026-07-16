<script setup lang="ts">
import { computed } from "vue"
import VChart from "vue-echarts"
import "echarts"
import { usePreferredDark } from "@vueuse/core"
import type { TrendsResponse, RangeKey } from "../types/api"
import { useI18n } from "../composables/useI18n"

const props = defineProps<{
  data: TrendsResponse | null
  range: RangeKey
  error?: string | null
}>()
const emit = defineEmits<{ "update:range": [range: RangeKey] }>()

const { t, d } = useI18n()
const isDark = usePreferredDark()

const RANGE_SECS: Record<RangeKey, number> = {
  "24h": 86400,
  "3d": 259200,
  "7d": 604800,
}

function filterHistory() {
  const history = props.data?.history ?? []
  if (!history.length) return history
  const cutoff = Math.floor(Date.now() / 1000) - RANGE_SECS[props.range]
  return history.filter((item) => item.timestamp >= cutoff)
}

const option = computed(() => {
  const history = filterHistory()
  const dark = isDark.value
  const cc = dark
    ? { axis: "#94a3b8", line: "#334155", legend: "#cbd5e1" }
    : { axis: "#64748b", line: "#e6ebf2", legend: "#1f2937" }

  if (!history.length) {
    return {
      title: {
        text: props.error ? t("top100_error") : t("top100_empty"),
        left: "center",
        top: "center",
        textStyle: { color: "#94a3b8", fontSize: 14 },
      },
      series: [],
    }
  }

  const labels = history.map((item) => d(item.timestamp * 1000, "chart"))

  return {
    title: { text: "" },
    color: dark ? ["#3b82f6", "#94a3b8", "#22c55e", "#f59e0b"] : ["#2563eb", "#475569", "#15803d", "#b45309"],
    tooltip: {
      trigger: "axis",
      backgroundColor: dark ? "#1e293b" : "#ffffff",
      borderColor: dark ? "#334155" : "#e2e8f0",
      textStyle: { color: dark ? "#e2e8f0" : "#1f2937" },
    },
    legend: {
      type: "scroll",
      top: 0,
      left: "center",
      itemWidth: 16,
      itemGap: 14,
      textStyle: { fontSize: 11, color: cc.legend },
      data: [t("torrents"), t("sort_peers"), t("sort_seeders"), t("sort_leechers")],
    },
    grid: { left: 4, right: 4, top: 52, bottom: 36, containLabel: true },
    xAxis: {
      type: "category",
      boundaryGap: false,
      data: labels,
      axisLine: { lineStyle: { color: cc.line } },
      axisLabel: { color: cc.axis },
    },
    yAxis: {
      type: "value",
      minInterval: 1,
      axisLabel: { color: cc.axis },
      splitLine: { lineStyle: { color: cc.line } },
    },
    series: [
      { name: t("torrents"), type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.torrents) },
      { name: t("sort_peers"), type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.peers) },
      { name: t("sort_seeders"), type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.seeders) },
      { name: t("sort_leechers"), type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.leechers) },
    ],
  }
})

function setRange(r: RangeKey) {
  emit("update:range", r)
}

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
        :model-value="range"
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
        @update:model-value="setRange"
      />
    </div>
    <div class="w-full h-[440px] max-[900px]:h-[330px] max-[560px]:h-[275px]">
      <v-chart :option="option" :init-options="{ renderer: 'svg' }" autoresize />
    </div>
  </section>
</template>
