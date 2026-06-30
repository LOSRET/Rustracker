<script setup lang="ts">
import type { StatsResponse } from "../types/api";
import { useI18n } from "../composables/useI18n";

const props = defineProps<{
  stats: StatsResponse | null;
  error: string | null;
}>();

const { t, number } = useI18n();
</script>

<template>
  <section class="mb-6">
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div>
        <h1 class="text-2xl font-display font-bold">{{ t.title }}</h1>
        <p class="text-sm text-slate-500 dark:text-slate-400 mt-1">{{ t.subtitle }}</p>
        <p
          :class="['text-sm mt-2', error ? 'text-red-500' : 'text-green-600 dark:text-green-400']"
        >
          {{ error ? t.error : t.running }}
        </p>
        <div class="mt-2 text-sm">
          <span class="text-slate-500">{{ t.tracker_addr_label }}</span>
          <code class="cursor-pointer hover:text-blue-600" title="Copy">{{ t.config_fmt() }}</code>
        </div>
      </div>
      <div class="text-right">
        <div class="text-xs text-slate-500 uppercase tracking-wide">RPS</div>
        <strong class="text-3xl font-display tabular-nums">{{ number(props.stats?.rps ?? 0) }}</strong>
      </div>
    </div>
  </section>
</template>
