<script setup lang="ts">
import type { StatsResponse } from "../types/api";
import { useTrends } from "../composables/useTrends";
import { useI18n } from "../composables/useI18n";
import Topbar from "../components/Topbar.vue";
import MetricCard from "../components/MetricCard.vue";
import TrendChart from "../components/TrendChart.vue";
import ClientChart from "../components/ClientChart.vue";

defineProps<{ stats: StatsResponse | null; error: string | null }>();

const { trends, clients } = useTrends();
const { number } = useI18n();
</script>

<template>
  <div>
    <Topbar :stats="stats" :error="error" />

    <section
      class="grid gap-3 mb-5"
      style="grid-template-columns: repeat(5, minmax(140px, 1fr))"
    >
      <MetricCard variant="peers" label="Peers" :value="number(stats?.peers ?? 0)" />
      <MetricCard variant="seeders" label="Seeders" :value="number(stats?.seeders ?? 0)" />
      <MetricCard variant="leechers" label="Leechers" :value="number(stats?.leechers ?? 0)" />
      <MetricCard variant="torrents" label="Torrents" :value="number(stats?.torrents ?? 0)" />
      <MetricCard variant="completed" label="Completed" :value="number(stats?.completed ?? 0)" />
    </section>

    <TrendChart :data="trends" />
    <ClientChart :data="clients" />
  </div>
</template>

<style scoped>
@media (max-width: 900px) {
  section {
    grid-template-columns: repeat(2, minmax(130px, 1fr)) !important;
  }
}
@media (max-width: 560px) {
  section {
    grid-template-columns: 1fr !important;
  }
}
</style>
