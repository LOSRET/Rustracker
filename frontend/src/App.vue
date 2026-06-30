<script setup lang="ts">
import { ref, onMounted } from "vue";
import type { PageKey } from "./types/api";
import { useI18n } from "./composables/useI18n";
import { useStats } from "./composables/useStats";
import Sidebar from "./components/Sidebar.vue";
import AppFooter from "./components/AppFooter.vue";
import Disclaimer from "./components/Disclaimer.vue";
import DashboardView from "./views/DashboardView.vue";
import Top100Page from "./components/Top100Page.vue";

const { setLang } = useI18n();
const { stats, error } = useStats();
const page = ref<PageKey>("dashboard");
const sidebarOpen = ref(false);

function switchPage(p: PageKey) {
  page.value = p;
  sidebarOpen.value = false;
}

onMounted(() => setLang("zh"));
</script>

<template>
  <button
    class="fixed top-4 left-4 z-50 md:hidden p-2 rounded bg-slate-800 text-white"
    @click="sidebarOpen = !sidebarOpen"
    aria-label="Menu"
  >
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <line x1="3" y1="6" x2="21" y2="6" />
      <line x1="3" y1="12" x2="21" y2="12" />
      <line x1="3" y1="18" x2="21" y2="18" />
    </svg>
  </button>

  <div
    v-if="sidebarOpen"
    class="fixed inset-0 bg-black/50 z-40 md:hidden"
    @click="sidebarOpen = false"
  />

  <div class="flex min-h-screen bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100">
    <Sidebar :page="page" @switch="switchPage" :open="sidebarOpen" />
    <main class="flex-1 min-w-0 p-4 md:p-8 max-w-7xl">
      <DashboardView v-if="page === 'dashboard'" :stats="stats" :error="error" />
      <Top100Page v-else-if="page === 'top100'" />
      <Disclaimer />
      <AppFooter :stats="stats" />
    </main>
  </div>
</template>
