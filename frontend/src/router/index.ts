import { createRouter, createWebHashHistory } from "vue-router"
import DashboardView from "../views/DashboardView.vue"
import Top100Page from "../views/Top100Page.vue"
import ClientsPage from "../views/ClientsPage.vue"

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", name: "dashboard", component: DashboardView },
    { path: "/top100", name: "top100", component: Top100Page },
    { path: "/clients", name: "clients", component: ClientsPage },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
})
