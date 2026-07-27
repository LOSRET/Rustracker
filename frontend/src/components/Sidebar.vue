<script setup lang="ts">
import { DialogRoot, DialogPortal, DialogOverlay, DialogContent } from "reka-ui"
import { useMediaQuery } from "@vueuse/core"
import type { PageKey } from "../types/api"
import SidebarContent from "./SidebarContent.vue"

defineProps<{ page: PageKey; open: boolean; error?: string | null }>()
const emit = defineEmits<{ switch: [page: PageKey]; close: [] }>()

const isMobile = useMediaQuery("(max-width: 900px)")
</script>

<template>
  <aside v-if="!isMobile" class="bg-side text-side-fg sticky top-0 h-screen overflow-y-auto p-6">
    <SidebarContent :page="page" :error="error" @switch="emit('switch', $event)" />
  </aside>

  <DialogRoot
    v-else
    :open="open"
    @update:open="
      (v: boolean) => {
        if (!v) emit('close')
      }
    "
  >
    <DialogPortal>
      <DialogOverlay class="fixed inset-0 bg-black/45 z-[1099]" />
      <DialogContent
        class="fixed top-0 left-0 h-screen w-[260px] bg-side text-side-fg overflow-y-auto p-5 focus:outline-none z-[1100]"
      >
        <SidebarContent :page="page" :error="error" @switch="emit('switch', $event)" />
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
