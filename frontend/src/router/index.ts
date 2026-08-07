import { createRouter, createWebHistory } from "vue-router"
import DashboardView from "../views/DashboardView.vue"
import Top100Page from "../views/Top100Page.vue"
import ClientsPage from "../views/ClientsPage.vue"

export const router = createRouter({
  history: createWebHistory(),
  scrollBehavior(_to, _from, savedPosition) {
    return savedPosition || { top: 0 }
  },
  routes: [
    { path: "/", name: "dashboard", component: DashboardView },
    { path: "/top100", name: "top100", component: Top100Page },
    { path: "/clients", name: "clients", component: ClientsPage },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
})
