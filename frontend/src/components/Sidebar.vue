<script setup lang="ts">
import { Dialog, DialogPanel, TransitionRoot, TransitionChild } from "@headlessui/vue";
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

  <TransitionRoot v-else :show="open" appear as="template">
    <Dialog @close="emit('close')" class="relative z-[1100]">
      <TransitionChild
        as="template"
        enter="duration-200 ease-out"
        enterFrom="opacity-0"
        enterTo="opacity-100"
        leave="duration-200 ease-in"
        leaveFrom="opacity-100"
        leaveTo="opacity-0"
      >
        <div class="fixed inset-0 bg-black/45" />
      </TransitionChild>

      <TransitionChild
        as="template"
        enter="transition duration-200 ease-out"
        enterFrom="-translate-x-full"
        enterTo="translate-x-0"
        leave="transition duration-200 ease-in"
        leaveFrom="translate-x-0"
        leaveTo="-translate-x-full"
      >
        <DialogPanel class="fixed top-0 left-0 h-screen w-[260px] bg-side text-[#f8fafc] overflow-y-auto p-5">
          <SidebarContent :page="page" :error="error" @switch="emit('switch', $event)" />
        </DialogPanel>
      </TransitionChild>
    </Dialog>
  </TransitionRoot>
</template>
