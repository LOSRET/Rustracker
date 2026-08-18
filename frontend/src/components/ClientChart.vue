<script setup lang="ts">
import { computed } from "vue"
import VChart from "vue-echarts"
import { usePreferredDark } from "@vueuse/core"
import type { ClientsResponse, RangeKey } from "../types/api"
import { useI18n } from "../composables/useI18n"
import { baseChart, emptyChartOption, filterRange, lineSeries } from "../utils/chart"

const {
  data,
  range,
  error = null,
} = defineProps<{
  data: ClientsResponse | null
  range: RangeKey
  error?: string | null
}>()

const { t, d } = useI18n()
const isDark = usePreferredDark()

const CLIENT_BRAND: Record<string, string> = {
  Xunlei: "#1976D2",
  迅雷: "#1976D2",
  qBittorrent: "#2196F3",
  Transmission: "#D32F2F",
  Deluge: "#388E3C",
  uTorrent: "#7CB342",
  µTorrent: "#7CB342",
  BitComet: "#FF8F00",
  BiglyBT: "#00897B",
  Vuze: "#1565C0",
  aria2: "#455A64",
  libTorrent: "#7E57C2",
  BitTorrent: "#4CAF50",
  rTorrent: "#C62828",
  Tixati: "#E65100",
  WebTorrent: "#00ACC1",
  FrostWire: "#0097A7",
  ktorrent: "#1E88E5",
  LibreTorrent: "#43A047",
  Flud: "#26A69A",
  Motrix: "#6D28D9",
  Picotorrent: "#66BB6A",
}
const UNKNOWN_GRAY = "#9E9E9E"

function brandColor(name: string): string {
  const trimmed = name.trim()
  if (!trimmed || /^unknown$/i.test(trimmed)) return UNKNOWN_GRAY
  const lower = trimmed.toLowerCase()
  for (const [key, color] of Object.entries(CLIENT_BRAND)) {
    if (lower.includes(key.toLowerCase())) return color
  }
  let h = 0
  for (let i = 0; i < trimmed.length; i++) h = ((h << 5) - h + trimmed.charCodeAt(i)) | 0
  return `hsl(${Math.abs(h) % 360}, 65%, 50%)`
}

const option = computed(() => {
  const historyAll = data?.history ?? []
  if (!historyAll.length) {
    return emptyChartOption(error ? t("top100_error") : t("top100_empty"))
  }

  const names = data?.clients ?? []
  const history = filterRange(historyAll, range)
  const labels = history.map((item) => d(item.timestamp * 1000, "chart"))

  return {
    title: { text: "" },
    ...baseChart(isDark.value, names, labels),
    series: names.map((name, j) =>
      lineSeries(
        name,
        history.map((item) => item.counts[j] ?? 0),
        { itemStyle: { color: brandColor(name) } },
      ),
    ),
  }
})
</script>

<template>
  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-baseline justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div>
        <h2 class="m-0 text-base leading-relaxed font-bold">{{ t("client_chart_title") }}</h2>
        <span class="text-muted text-xs">{{ t("client_chart_note") }}</span>
      </div>
    </div>
    <div class="w-full h-[440px] max-[900px]:h-[330px] max-[560px]:h-[275px]">
      <v-chart :option="option" :init-options="{ renderer: 'svg' }" autoresize />
    </div>
  </section>
</template>
