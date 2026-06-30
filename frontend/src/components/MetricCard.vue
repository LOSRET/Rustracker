<script setup lang="ts">
defineProps<{
  label: string;
  value: string | number;
  variant?: "peers" | "seeders" | "leechers" | "torrents" | "completed";
}>();

const borderMap: Record<string, string> = {
  peers: "border-t-peers",
  seeders: "border-t-good",
  leechers: "border-t-warn",
  torrents: "border-t-accent",
  completed: "border-t-violet",
};
</script>

<template>
  <div :class="['bg-panel border border-line border-t-4 p-4 min-h-[104px]', borderMap[variant ?? 'peers']]">
    <span class="block text-muted text-xs uppercase mb-2.5">{{ label }}</span>
    <strong class="block text-[30px] leading-none">{{ value }}</strong>
  </div>
</template>
