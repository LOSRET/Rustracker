<script setup lang="ts">
import { useI18n } from "../composables/useI18n";
const { t } = useI18n();
const contactHtml = __CONTACT_HTML__;
</script>

<template>
  <section id="disclaimer" class="bg-panel border border-line p-[18px] mb-1">
    <h2 class="m-0 mb-3 text-base leading-relaxed">{{ t.disc_title }}</h2>
    <p class="m-0 mb-2.5 text-muted leading-[1.7]">{{ t.disc_p1 }}</p>
    <p class="m-0 mb-2.5 text-muted leading-[1.7]">{{ t.disc_p2 }}</p>
    <p class="m-0 mb-2.5 text-muted leading-[1.7]">{{ t.disc_p3 }}</p>
    <p class="m-0 mb-2.5 text-muted leading-[1.7]">{{ t.disc_p4 }}</p>
    <p class="m-0 text-muted leading-[1.7]">{{ t.disc_p5 }}</p>
    <div
      v-if="contactHtml"
      v-html="contactHtml"
      class="mt-4 text-center text-lg font-bold leading-[1.8] [&_a]:text-accent [&_a]:no-underline"
    ></div>
  </section>
</template>
