<script setup lang="ts">
import { useI18n } from "../composables/useI18n";
const { t } = useI18n();
const contact = __CONTACT__;
</script>

<template>
  <section id="disclaimer" class="bg-panel border border-line p-[18px] mb-1">
    <h2 class="m-0 mb-3 text-base leading-relaxed font-bold">{{ t.disc_title }}</h2>
    <p class="m-0 mb-2.5 text-muted leading-[1.7]">{{ t.disc_p1 }}</p>
    <p class="m-0 mb-2.5 text-muted leading-[1.7]">{{ t.disc_p2 }}</p>
    <p class="m-0 mb-2.5 text-muted leading-[1.7]">{{ t.disc_p3 }}</p>
    <p class="m-0 mb-2.5 text-muted leading-[1.7]">{{ t.disc_p4 }}</p>
    <p class="m-0 text-muted leading-[1.7]">{{ t.disc_p5 }}</p>
    <div
      v-if="contact"
      class="mt-4 text-center text-lg font-bold leading-[1.8]"
    >
      <div>
        {{ t.blog_label }}：<a
          :href="contact.blogUrl"
          rel="noopener noreferrer"
          target="_blank"
          class="text-accent no-underline"
          >{{ contact.blogUrl }}</a
        >
      </div>
      <div>
        {{ t.contact_label }}：<a
          :href="`mailto:${contact.email}`"
          class="text-accent no-underline"
          >{{ contact.email }}</a
        >
      </div>
    </div>
  </section>
</template>
