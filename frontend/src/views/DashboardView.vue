<script setup lang="ts">
import { ref } from "vue";
import type { StatsResponse, RangeKey } from "../types/api";
import { useTrends } from "../composables/useTrends";
import { useI18n } from "../composables/useI18n";
import Topbar from "../components/Topbar.vue";
import MetricCard from "../components/MetricCard.vue";
import TrendChart from "../components/TrendChart.vue";
import ClientChart from "../components/ClientChart.vue";

defineProps<{ stats: StatsResponse | null; error: string | null; lastUpdated: number | null }>();

const { trends, clients } = useTrends();
const { t, number } = useI18n();
const range = ref<RangeKey>("24h");
</script>

<template>
  <div>
    <Topbar :stats="stats" :error="error" :last-updated="lastUpdated" />

    <section
      class="grid gap-3 mb-5 grid-cols-[repeat(5,minmax(140px,1fr))] max-[1100px]:grid-cols-[repeat(3,minmax(140px,1fr))] max-[900px]:grid-cols-[repeat(2,minmax(130px,1fr))] max-[560px]:grid-cols-1"
    >
      <MetricCard variant="peers" :label="t.sort_peers" :value="number(stats?.peers ?? 0)" />
      <MetricCard variant="seeders" :label="t.sort_seeders" :value="number(stats?.seeders ?? 0)" />
      <MetricCard variant="leechers" :label="t.sort_leechers" :value="number(stats?.leechers ?? 0)" />
      <MetricCard variant="torrents" :label="t.torrents" :value="number(stats?.torrents ?? 0)" />
      <MetricCard variant="completed" :label="t.completed" :value="number(stats?.completed ?? 0)" />
    </section>

    <TrendChart :data="trends" v-model:range="range" />
    <ClientChart :data="clients" :range="range" />
  </div>
</template>
