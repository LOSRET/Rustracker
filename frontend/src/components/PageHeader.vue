<script setup lang="ts">
defineProps<{ title: string; subtitle: string }>()
</script>

<template>
  <section class="flex justify-between items-start gap-5 mb-6 max-[900px]:flex-col max-[900px]:items-stretch">
    <div>
      <h1 class="m-0 mb-1.5 text-[28px] leading-tight max-[560px]:text-[24px] font-bold">{{ title }}</h1>
      <p class="m-0 text-muted text-sm leading-relaxed">{{ subtitle }}</p>
      <slot />
    </div>
    <slot name="actions" />
  </section>
</template>
