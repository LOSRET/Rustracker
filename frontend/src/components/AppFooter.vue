<script setup lang="ts">
import type { StatsResponse } from "../types/api"
import { useI18n } from "../composables/useI18n"

defineProps<{ stats: StatsResponse | null }>()
const { t } = useI18n()

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (d > 0) return `${d}d ${h}h ${m}m`
  if (h > 0) return `${h}h ${m}m`
  return `${m}m`
}
</script>

<template>
  <footer class="flex justify-end items-center flex-wrap gap-6 py-4 text-muted text-xs">
    <span>
      {{ t("powered_by") }}
      <a
        href="https://github.com/LOSRET/Rustracker"
        target="_blank"
        rel="noopener noreferrer"
        class="text-accent no-underline font-semibold hover:underline"
        >Rustracker</a
      >
      v{{ stats?.version ?? "-" }}
    </span>
    <span v-if="stats?.uptime_secs != null">{{ t("uptime") }} {{ formatUptime(stats.uptime_secs) }}</span>
  </footer>
</template>
