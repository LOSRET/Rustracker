<script setup lang="ts">
import { useI18n } from "../composables/useI18n";
const { t } = useI18n();
</script>

<template>
  <section id="disclaimer" class="mt-12 p-6 rounded-lg bg-slate-100 dark:bg-slate-900/50 text-sm leading-relaxed">
    <h2 class="text-lg font-display font-bold mb-3">{{ t.disc_title }}</h2>
    <p class="mb-2 text-slate-600 dark:text-slate-400">{{ t.disc_p1 }}</p>
    <p class="mb-2 text-slate-600 dark:text-slate-400">{{ t.disc_p2 }}</p>
    <p class="mb-2 text-slate-600 dark:text-slate-400">{{ t.disc_p3 }}</p>
    <p class="mb-2 text-slate-600 dark:text-slate-400">{{ t.disc_p4 }}</p>
    <p class="text-slate-600 dark:text-slate-400">{{ t.disc_p5 }}</p>
  </section>
</template>
