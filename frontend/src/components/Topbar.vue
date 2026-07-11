<script setup lang="ts">
import type { StatsResponse } from "../types/api";
import { useClipboard } from "@vueuse/core";
import { useI18n } from "../composables/useI18n";

const props = defineProps<{
  stats: StatsResponse | null;
  error: string | null;
  lastUpdated: number | null;
}>();

const { t, number, d } = useI18n();
const { copied, copy } = useClipboard({ copiedDuring: 1500, legacy: true });

const trackerUrl = `${window.location.origin}/announce`;

function copyAddr() {
  copy(trackerUrl);
}

function lastUpdateText() {
  const ts = props.lastUpdated;
  if (ts == null) return t("loading");
  return `${t("last_update")} ${d(ts, "time")}`;
}
</script>

<template>
  <section class="flex justify-between items-start gap-5 mb-6 max-[900px]:flex-col max-[900px]:items-stretch">
    <div>
      <h1 class="m-0 mb-1.5 text-[28px] leading-tight font-bold max-[560px]:text-[24px]">{{ t('title') }}</h1>
      <p class="m-0 text-muted text-sm leading-relaxed">{{ t('subtitle') }}</p>
      <p :class="['text-xs mt-1.5', error ? 'text-bad' : 'text-muted']">
        {{ error ? t('error') : lastUpdateText() }}
      </p>
      <div class="flex items-center flex-wrap gap-1 mb-5">
        <span class="text-muted text-sm font-normal whitespace-nowrap">{{ t('tracker_addr_label') }}</span>
        <span
          :class="[
            'tracker-addr text-base font-bold break-all cursor-pointer relative border-b border-dashed border-line transition-colors',
            copied ? 'tracker-addr-copied text-good' : 'text-ink hover:text-accent',
          ]"
          :data-tooltip="t('copied')"
          @click="copyAddr"
        >
          {{ trackerUrl }}
        </span>
      </div>
    </div>
    <div class="text-right shrink-0 bg-soft px-3.5 py-2 rounded max-[900px]:text-left">
      <div class="flex items-center justify-end gap-1.5 text-muted text-[11px] uppercase mb-0.5 max-[900px]:justify-start">
        <i class="w-[7px] h-[7px] bg-good rounded-full shrink-0 animate-rps-pulse"></i>
        RPS
      </div>
      <strong class="text-[22px] leading-none text-ink max-[900px]:text-xl">{{ number(props.stats?.rps ?? 0) }}</strong>
    </div>
  </section>
</template>

<style scoped>
.tracker-addr-copied::after {
  content: attr(data-tooltip);
  position: absolute;
  top: -28px;
  left: 50%;
  transform: translateX(-50%);
  background: #1f2937;
  color: #fff;
  font-size: 12px;
  font-weight: 500;
  padding: 3px 10px;
  border-radius: 4px;
  white-space: nowrap;
  pointer-events: none;
}
@media (prefers-color-scheme: dark) {
  .tracker-addr-copied::after {
    background: #e2e8f0;
    color: #0f172a;
  }
}
</style>
