<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue"
import { useRoute } from "vue-router"
import type { PageKey } from "./types/api"
import { useSeoHead } from "./composables/useI18n"
import { useStats } from "./composables/useStats"
import Sidebar from "./components/Sidebar.vue"
import AppFooter from "./components/AppFooter.vue"
import Disclaimer from "./components/Disclaimer.vue"

const { stats, error, lastUpdated, stop } = useStats()
const sidebarOpen = ref(false)
const route = useRoute()
const page = computed(() => route.name as PageKey)
const dashboardProps = computed(() => (route.name === "dashboard" ? { stats, error, lastUpdated } : {}))

useSeoHead()

watch(
  () => route.fullPath,
  () => {
    sidebarOpen.value = false
  },
)

onUnmounted(stop)
</script>

<template>
  <UApp>
    <button
      class="hidden max-[900px]:block fixed top-[14px] left-[14px] z-[999] bg-side text-side-fg border-0 p-2 cursor-pointer rounded-md leading-none"
      aria-label="Menu"
      @click="sidebarOpen = !sidebarOpen"
    >
      <UIcon name="i-lucide-menu" class="size-6" />
    </button>

    <div class="min-h-screen grid grid-cols-[248px_minmax(0,1fr)] max-[900px]:grid-cols-1">
      <Sidebar :page="page" :open="sidebarOpen" :error="error" @close="sidebarOpen = false" />
      <main class="min-w-0 p-7 max-[900px]:pt-[60px] max-[900px]:px-[18px] max-[900px]:pb-[18px]">
        <RouterView v-slot="{ Component }">
          <KeepAlive>
            <component :is="Component" v-bind="dashboardProps" />
          </KeepAlive>
        </RouterView>
        <Disclaimer />
        <AppFooter :stats="stats" />
      </main>
    </div>
  </UApp>
</template>
