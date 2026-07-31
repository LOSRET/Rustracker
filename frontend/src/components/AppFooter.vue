<script setup lang="ts">
import dayjs from "dayjs"
import duration from "dayjs/plugin/duration"
import type { StatsResponse } from "../types/api"
import { useI18n } from "../composables/useI18n"

dayjs.extend(duration)

defineProps<{ stats: StatsResponse | null }>()
const { t } = useI18n()

function formatUptime(secs: number): string {
  const dur = dayjs.duration(secs, "seconds")
  const d = dur.days()
  const h = dur.hours()
  const m = dur.minutes()
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
