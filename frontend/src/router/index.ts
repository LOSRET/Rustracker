import { createRouter, createWebHistory } from "vue-router"

export const router = createRouter({
  history: createWebHistory(),
  scrollBehavior(_to, _from, savedPosition) {
    return savedPosition || { top: 0 }
  },
  routes: [
    { path: "/", name: "dashboard", component: () => import("../views/DashboardView.vue") },
    { path: "/top100", name: "top100", component: () => import("../views/Top100Page.vue") },
    { path: "/clients", name: "clients", component: () => import("../views/ClientsPage.vue") },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
})
