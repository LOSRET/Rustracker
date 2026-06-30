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
  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-baseline justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div class="flex items-center flex-wrap gap-3">
        <h2 class="m-0 text-base leading-relaxed">{{ t.chart_title }}</h2>
        <span class="text-muted text-xs">{{ t.chart_note }}</span>
      </div>
      <div class="flex shrink-0">
        <button
          v-for="(r, i) in (['24h', '3d', '7d'] as RangeKey[])"
          :key="r"
          :class="[
            'border text-muted px-3 text-xs cursor-pointer min-h-7 transition-colors',
            i === 0 ? 'rounded-l' : 'border-l-0',
            i === 2 ? 'rounded-r' : '',
            range === r
              ? 'bg-accent border-accent text-white'
              : 'bg-panel border-line hover:bg-hover-soft',
          ]"
          @click="range = r"
        >
          {{ t[`range_${r}` as keyof typeof t] }}
        </button>
      </div>
    </div>
    <div ref="chartEl" class="w-full h-[440px] max-[900px]:h-[330px] max-[560px]:h-[275px]" />
  </section>
</template>
