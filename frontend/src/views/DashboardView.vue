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

    <section class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4 mb-6">
      <MetricCard variant="peers" :label="'Peers'" :value="number(stats?.peers ?? 0)" />
      <MetricCard variant="seeders" :label="'Seeders'" :value="number(stats?.seeders ?? 0)" />
      <MetricCard variant="leechers" :label="'Leechers'" :value="number(stats?.leechers ?? 0)" />
      <MetricCard variant="torrents" :label="'Torrents'" :value="number(stats?.torrents ?? 0)" />
      <MetricCard variant="completed" :label="'Completed'" :value="number(stats?.completed ?? 0)" />
    </section>

    <TrendChart :data="trends" />
    <ClientChart :data="clients" />
  </div>
</template>
