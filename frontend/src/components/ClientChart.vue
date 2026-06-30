<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from "vue";
import * as echarts from "echarts";
import type { ClientsResponse } from "../types/api";
import { useI18n } from "../composables/useI18n";

const props = defineProps<{
  data: ClientsResponse | null;
}>();

const { t, localeFor } = useI18n();
const chartEl = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;

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
};

function brandColor(name: string): string {
  for (const [key, color] of Object.entries(CLIENT_BRAND)) {
    if (name.includes(key)) return color;
  }
  return "#9E9E9E";
}

function isDark() {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function render() {
  if (!chart) return;
  const history = props.data?.history ?? [];
  const dark = isDark();
  const cc = dark
    ? { axis: "#94a3b8", line: "#334155", legend: "#cbd5e1" }
    : { axis: "#64748b", line: "#e6ebf2", legend: "#1f2937" };

  if (!history.length) {
    chart.setOption({ title: { text: t.value.top100_empty, left: "center", top: "center", textStyle: { color: "#94a3b8", fontSize: 14 } }, series: [] });
    return;
  }

  const tags = props.data?.tags ?? [];
  const labels = history.map((item) =>
    new Date(item.timestamp * 1000).toLocaleString(localeFor(), {
      month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false,
    }),
  );

  chart.setOption({
    tooltip: {
      trigger: "axis",
      backgroundColor: dark ? "#1e293b" : "#ffffff",
      borderColor: dark ? "#334155" : "#e2e8f0",
      textStyle: { color: dark ? "#e2e8f0" : "#1f2937" },
    },
    legend: {
      type: "scroll", top: 0, left: "center", itemWidth: 16, itemGap: 14,
      textStyle: { fontSize: 11, color: cc.legend },
      data: tags,
    },
    grid: { left: 4, right: 4, top: 52, bottom: 36, containLabel: true },
    xAxis: { type: "category", boundaryGap: false, data: labels, axisLine: { lineStyle: { color: cc.line } }, axisLabel: { color: cc.axis } },
    yAxis: { type: "value", minInterval: 1, axisLabel: { color: cc.axis }, splitLine: { lineStyle: { color: cc.line } } },
    series: tags.map((tag) => ({
      name: tag,
      type: "line",
      smooth: true,
      showSymbol: false,
      itemStyle: { color: brandColor(tag) },
      data: history.map((item) => item.counts[tag] ?? 0),
    })),
  });
}

function onResize() {
  chart?.resize();
}

onMounted(() => {
  if (chartEl.value) chart = echarts.init(chartEl.value);
  render();
  window.addEventListener("resize", onResize);
});

onUnmounted(() => {
  window.removeEventListener("resize", onResize);
  chart?.dispose();
});

watch(() => props.data, render, { deep: true });
</script>

<template>
  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-baseline justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div>
        <h2 class="m-0 text-base leading-relaxed">{{ t.client_chart_title }}</h2>
        <span class="text-muted text-xs">{{ t.client_chart_note }}</span>
      </div>
    </div>
    <div ref="chartEl" class="w-full h-[440px] max-[900px]:h-[330px] max-[560px]:h-[275px]" />
  </section>
</template>
