<script setup lang="ts">
import type { StatsResponse } from "../types/api";

defineProps<{ stats: StatsResponse | null }>();

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}
</script>

<template>
  <footer class="mt-8 py-6 border-t border-slate-200 dark:border-slate-800 text-sm text-slate-500 flex flex-wrap justify-between gap-2">
    <span>
      Powered by
      <a href="https://github.com/LOSRET/Rustracker" target="_blank" rel="noopener noreferrer" class="text-blue-600 hover:underline">Rustracker</a>
      v{{ stats?.version ?? "-" }}
    </span>
    <span v-if="stats?.uptime_secs != null">Uptime: {{ formatUptime(stats.uptime_secs) }}</span>
  </footer>
</template>
