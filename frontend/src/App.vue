<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue"
import type { PageKey } from "./types/api"
import { useI18n } from "./composables/useI18n"
import { useStats } from "./composables/useStats"
import { detectLang } from "./i18n"
import Sidebar from "./components/Sidebar.vue"
import AppFooter from "./components/AppFooter.vue"
import Disclaimer from "./components/Disclaimer.vue"
import DashboardView from "./views/DashboardView.vue"
import Top100Page from "./components/Top100Page.vue"
import ClientsPage from "./components/ClientsPage.vue"

const { setLang } = useI18n()
const { stats, error, lastUpdated, stop } = useStats()
const page = ref<PageKey>("dashboard")
const sidebarOpen = ref(false)

function switchPage(p: PageKey) {
  if (p !== page.value) {
    page.value = p
    window.scrollTo(0, 0)
  }
  sidebarOpen.value = false
}

onMounted(() => setLang(detectLang()))
onUnmounted(stop)
</script>

<template>
  <UApp>
    <button
      class="hidden max-[900px]:block fixed top-[14px] left-[14px] z-[999] bg-side text-side-fg border-0 p-2 cursor-pointer rounded-md leading-none"
      aria-label="Menu"
      @click="sidebarOpen = !sidebarOpen"
    >
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <line x1="3" y1="6" x2="21" y2="6" />
        <line x1="3" y1="12" x2="21" y2="12" />
        <line x1="3" y1="18" x2="21" y2="18" />
      </svg>
    </button>

    <div class="min-h-screen grid grid-cols-[248px_minmax(0,1fr)] max-[900px]:grid-cols-1">
      <Sidebar :page="page" :open="sidebarOpen" :error="error" @switch="switchPage" @close="sidebarOpen = false" />
      <main class="min-w-0 p-7 max-[900px]:pt-[60px] max-[900px]:px-[18px] max-[900px]:pb-[18px]">
        <KeepAlive>
          <DashboardView v-if="page === 'dashboard'" :stats="stats" :error="error" :last-updated="lastUpdated" />
          <Top100Page v-else-if="page === 'top100'" />
          <ClientsPage v-else-if="page === 'clients'" />
        </KeepAlive>
        <Disclaimer />
        <AppFooter :stats="stats" />
      </main>
    </div>
  </UApp>
</template>
