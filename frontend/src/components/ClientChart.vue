<script setup lang="ts">
import { computed } from "vue";
import VChart from "vue-echarts";
import "echarts";
import { usePreferredDark } from "@vueuse/core";
import type { ClientsResponse, RangeKey } from "../types/api";
import { useI18n } from "../composables/useI18n";

const props = defineProps<{
  data: ClientsResponse | null;
  range: RangeKey;
}>();

const { t, localeFor } = useI18n();
const isDark = usePreferredDark();

const CLIENT_BRAND: Record<string, string> = {
  Xunlei: "#1976D2", "迅雷": "#1976D2",
  qBittorrent: "#2196F3",
  Transmission: "#D32F2F",
  Deluge: "#388E3C",
  uTorrent: "#7CB342", "µTorrent": "#7CB342",
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
};
const UNKNOWN_GRAY = "#9E9E9E";

const RANGE_SECS: Record<RangeKey, number> = {
  "24h": 86400,
  "3d": 259200,
  "7d": 604800,
};

function brandColor(name: string): string {
  const trimmed = name.trim();
  if (!trimmed || /^unknown$/i.test(trimmed)) return UNKNOWN_GRAY;
  const lower = trimmed.toLowerCase();
  for (const [key, color] of Object.entries(CLIENT_BRAND)) {
    if (lower.includes(key.toLowerCase())) return color;
  }
  let h = 0;
  for (let i = 0; i < trimmed.length; i++) h = ((h << 5) - h + trimmed.charCodeAt(i)) | 0;
  return `hsl(${Math.abs(h) % 360}, 65%, 50%)`;
}

const option = computed(() => {
  const historyAll = props.data?.history ?? [];
  const dark = isDark.value;
  const cc = dark
    ? { axis: "#94a3b8", line: "#334155", legend: "#cbd5e1" }
    : { axis: "#64748b", line: "#e6ebf2", legend: "#1f2937" };

  if (!historyAll.length) {
    return {
      title: { text: t("top100_empty"), left: "center", top: "center", textStyle: { color: "#94a3b8", fontSize: 14 } },
      series: [],
    };
  }

  const names = props.data?.clients ?? [];
  const cutoff = Math.floor(Date.now() / 1000) - RANGE_SECS[props.range];
  const history = historyAll.filter((item) => item.timestamp >= cutoff);
  const labels = history.map((item) =>
    new Date(item.timestamp * 1000).toLocaleString(localeFor(), {
      month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false,
    }),
  );

  return {
    title: { text: "" },
    tooltip: {
      trigger: "axis",
      backgroundColor: dark ? "#1e293b" : "#ffffff",
      borderColor: dark ? "#334155" : "#e2e8f0",
      textStyle: { color: dark ? "#e2e8f0" : "#1f2937" },
    },
    legend: {
      type: "scroll", top: 0, left: "center", itemWidth: 16, itemGap: 14,
      textStyle: { fontSize: 11, color: cc.legend },
      data: names,
    },
    grid: { left: 4, right: 4, top: 52, bottom: 36, containLabel: true },
    xAxis: { type: "category", boundaryGap: false, data: labels, axisLine: { lineStyle: { color: cc.line } }, axisLabel: { color: cc.axis } },
    yAxis: { type: "value", minInterval: 1, axisLabel: { color: cc.axis }, splitLine: { lineStyle: { color: cc.line } } },
    series: names.map((name, j) => ({
      name,
      type: "line",
      smooth: true,
      showSymbol: false,
      itemStyle: { color: brandColor(name) },
      data: history.map((item) => item.counts[j] ?? 0),
    })),
  };
});
</script>

<template>
  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-baseline justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div>
        <h2 class="m-0 text-base leading-relaxed font-bold">{{ t('client_chart_title') }}</h2>
        <span class="text-muted text-xs">{{ t('client_chart_note') }}</span>
      </div>
    </div>
    <div class="w-full h-[440px] max-[900px]:h-[330px] max-[560px]:h-[275px]">
      <v-chart :option="option" :init-options="{ renderer: 'svg' }" autoresize />
    </div>
  </section>
</template>
