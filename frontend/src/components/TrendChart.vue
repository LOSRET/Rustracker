<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted } from "vue";
import * as echarts from "echarts";
import type { TrendsResponse, RangeKey } from "../types/api";
import { useI18n } from "../composables/useI18n";

const props = defineProps<{
  data: TrendsResponse | null;
}>();

const { t, localeFor } = useI18n();
const range = ref<RangeKey>("24h");
const chartEl = ref<HTMLElement | null>(null);
let chart: echarts.ECharts | null = null;

const RANGE_SECS: Record<RangeKey, number> = {
  "24h": 86400,
  "3d": 259200,
  "7d": 604800,
};

function isDark() {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function filterHistory() {
  const history = props.data?.history ?? [];
  if (!history.length) return history;
  const cutoff = Math.floor(Date.now() / 1000) - RANGE_SECS[range.value];
  return history.filter((item) => item.timestamp >= cutoff);
}

function render() {
  if (!chart) return;
  const history = filterHistory();
  const dark = isDark();
  const cc = dark
    ? { axis: "#94a3b8", line: "#334155", legend: "#cbd5e1" }
    : { axis: "#64748b", line: "#e6ebf2", legend: "#1f2937" };

  if (!history.length) {
    chart.setOption({ title: { text: t.value.top100_empty, left: "center", top: "center", textStyle: { color: "#94a3b8", fontSize: 14 } }, series: [] });
    return;
  }

  const labels = history.map((item) =>
    new Date(item.timestamp * 1000).toLocaleString(localeFor(), {
      month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false,
    }),
  );

  chart.setOption({
    color: dark ? ["#3b82f6", "#94a3b8", "#22c55e", "#f59e0b"] : ["#2563eb", "#475569", "#15803d", "#b45309"],
    tooltip: {
      trigger: "axis",
      backgroundColor: dark ? "#1e293b" : "#ffffff",
      borderColor: dark ? "#334155" : "#e2e8f0",
      textStyle: { color: dark ? "#e2e8f0" : "#1f2937" },
    },
    legend: {
      type: "scroll", top: 0, left: "center", itemWidth: 16, itemGap: 14,
      textStyle: { fontSize: 11, color: cc.legend },
      data: ["Torrents", "Peers", "Seeders", "Leechers"],
    },
    grid: { left: 4, right: 4, top: 52, bottom: 36, containLabel: true },
    xAxis: { type: "category", boundaryGap: false, data: labels, axisLine: { lineStyle: { color: cc.line } }, axisLabel: { color: cc.axis } },
    yAxis: { type: "value", minInterval: 1, axisLabel: { color: cc.axis }, splitLine: { lineStyle: { color: cc.line } } },
    series: [
      { name: "Torrents", type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.torrents) },
      { name: "Peers", type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.peers) },
      { name: "Seeders", type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.seeders) },
      { name: "Leechers", type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.leechers) },
    ],
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

watch(() => [props.data, range.value], render, { deep: true });
</script>

<template>
  <section class="rounded-lg bg-white dark:bg-slate-900 p-6 shadow-sm mb-6">
    <div class="flex flex-wrap items-center gap-3 mb-4">
      <h2 class="text-lg font-display font-bold">{{ t.chart_title }}</h2>
      <span class="text-sm text-slate-500">{{ t.chart_note }}</span>
      <div class="flex gap-1 ml-auto">
        <button
          v-for="r in (['24h', '3d', '7d'] as RangeKey[])"
          :key="r"
          :class="['px-3 py-1 text-sm rounded transition-colors', range === r ? 'bg-blue-600 text-white' : 'bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700']"
          @click="range = r"
        >
          {{ t[`range_${r}` as keyof typeof t] }}
        </button>
      </div>
    </div>
    <div ref="chartEl" class="w-full h-80" />
  </section>
</template>
