<script setup lang="ts">
import { useMediaQuery } from "@vueuse/core";
import type { PageKey } from "../types/api";
import SidebarContent from "./SidebarContent.vue";

defineProps<{ page: PageKey; open: boolean; error?: string | null }>();
const emit = defineEmits<{ switch: [page: PageKey]; close: [] }>();

const isMobile = useMediaQuery("(max-width: 900px)");
</script>

<template>
  <aside v-if="!isMobile" class="bg-side text-[#f8fafc] sticky top-0 h-screen overflow-y-auto p-6">
    <SidebarContent :page="page" :error="error" @switch="emit('switch', $event)" />
  </aside>

  <USlideover
    v-else
    :open="open"
    side="left"
    :close="false"
    :dismissible="true"
    :ui="{
      overlay: 'fixed inset-0 bg-black/45',
      content: 'fixed top-0 left-0 h-screen w-[260px] bg-side text-[#f8fafc] overflow-y-auto p-5 focus:outline-none',
    }"
    @update:open="(v: boolean) => { if (!v) emit('close') }"
  >
    <template #content>
      <SidebarContent :page="page" :error="error" @switch="emit('switch', $event)" />
    </template>
  </USlideover>
</template>
