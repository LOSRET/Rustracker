<script setup lang="ts">
import { ref } from "vue";
import type { StatsResponse } from "../types/api";
import { useI18n } from "../composables/useI18n";

const props = defineProps<{
  stats: StatsResponse | null;
  error: string | null;
}>();

const { t, number, localeFor } = useI18n();
const copied = ref(false);

function copyAddr() {
  const text = t.value.config_fmt();
  navigator.clipboard.writeText(text).then(() => {
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  });
}

function lastUpdateText() {
  return `${t.value.last_update} ${new Date().toLocaleTimeString(localeFor())}`;
}
</script>

<template>
  <section class="flex justify-between items-start gap-5 mb-6 max-[900px]:flex-col max-[900px]:items-stretch">
    <div>
      <h1 class="m-0 mb-1.5 text-[28px] leading-tight max-[560px]:text-6xl">{{ t.title }}</h1>
      <p class="m-0 text-muted text-sm leading-relaxed">{{ t.subtitle }}</p>
      <p :class="['text-xs mt-1.5', error ? 'text-bad' : 'text-muted']">
        {{ error ? t.error : lastUpdateText() }}
      </p>
      <div class="flex items-center flex-wrap gap-1 mb-5">
        <span class="text-muted text-sm font-normal whitespace-nowrap">{{ t.tracker_addr_label }}</span>
        <span
          :class="[
            'text-base font-bold break-all cursor-pointer relative border-b border-dashed border-line transition-colors',
            copied ? 'text-good' : 'text-ink hover:text-accent',
          ]"
          :data-tooltip="t.copied"
          @click="copyAddr"
        >
          {{ t.config_fmt() }}
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
