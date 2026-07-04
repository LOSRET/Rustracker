<script setup lang="ts">
import { computed } from "vue";
import VChart from "vue-echarts";
import "echarts";
import { usePreferredDark } from "@vueuse/core";
import type { TrendsResponse, RangeKey } from "../types/api";
import { useI18n } from "../composables/useI18n";

const props = defineProps<{
  data: TrendsResponse | null;
  range: RangeKey;
}>();
const emit = defineEmits<{ "update:range": [range: RangeKey] }>();

const { t, localeFor } = useI18n();
const isDark = usePreferredDark();

const RANGE_SECS: Record<RangeKey, number> = {
  "24h": 86400,
  "3d": 259200,
  "7d": 604800,
};

function filterHistory() {
  const history = props.data?.history ?? [];
  if (!history.length) return history;
  const cutoff = Math.floor(Date.now() / 1000) - RANGE_SECS[props.range];
  return history.filter((item) => item.timestamp >= cutoff);
}

const option = computed(() => {
  const history = filterHistory();
  const dark = isDark.value;
  const cc = dark
    ? { axis: "#94a3b8", line: "#334155", legend: "#cbd5e1" }
    : { axis: "#64748b", line: "#e6ebf2", legend: "#1f2937" };
  const tr = t.value;

  if (!history.length) {
    return {
      title: { text: tr.top100_empty, left: "center", top: "center", textStyle: { color: "#94a3b8", fontSize: 14 } },
      series: [],
    };
  }

  const labels = history.map((item) =>
    new Date(item.timestamp * 1000).toLocaleString(localeFor(), {
      month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false,
    }),
  );

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
      type: "scroll", top: 0, left: "center", itemWidth: 16, itemGap: 14,
      textStyle: { fontSize: 11, color: cc.legend },
      data: [tr.torrents, tr.sort_peers, tr.sort_seeders, tr.sort_leechers],
    },
    grid: { left: 4, right: 4, top: 52, bottom: 36, containLabel: true },
    xAxis: { type: "category", boundaryGap: false, data: labels, axisLine: { lineStyle: { color: cc.line } }, axisLabel: { color: cc.axis } },
    yAxis: { type: "value", minInterval: 1, axisLabel: { color: cc.axis }, splitLine: { lineStyle: { color: cc.line } } },
    series: [
      { name: tr.torrents, type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.torrents) },
      { name: tr.sort_peers, type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.peers) },
      { name: tr.sort_seeders, type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.seeders) },
      { name: tr.sort_leechers, type: "line", smooth: true, showSymbol: false, data: history.map((i) => i.leechers) },
    ],
  };
});

function setRange(r: RangeKey) {
  emit("update:range", r);
}
</script>

<template>
  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-baseline justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div>
        <h2 class="m-0 text-base leading-relaxed font-bold">{{ t.chart_title }}</h2>
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
          @click="setRange(r)"
        >
          {{ t[`range_${r}` as keyof typeof t] }}
        </button>
      </div>
    </div>
    <div class="w-full h-[440px] max-[900px]:h-[330px] max-[560px]:h-[275px]">
      <v-chart :option="option" :init-options="{ renderer: 'svg' }" autoresize />
    </div>
  </section>
</template>
