<script setup lang="ts">
defineProps<{
  label: string;
  value: string | number;
  variant?: "default" | "peers" | "seeders" | "leechers" | "torrents" | "completed";
}>();

const variantClass: Record<string, string> = {
  peers: "border-blue-500",
  seeders: "border-green-500",
  leechers: "border-amber-500",
  torrents: "border-slate-500",
  completed: "border-purple-500",
  default: "border-slate-300 dark:border-slate-700",
};
</script>

<template>
  <div
    :class="[
      'rounded-lg bg-white dark:bg-slate-900 border-t-4 p-4 shadow-sm',
      variantClass[variant ?? 'default'],
    ]"
  >
    <span class="text-xs uppercase tracking-wide text-slate-500">{{ label }}</span>
    <strong class="block text-2xl font-display tabular-nums mt-1">{{ value }}</strong>
  </div>
</template>
