import { createApp } from "vue"
import { createHead } from "@unhead/vue/client"
import ui from "@nuxt/ui/vue-plugin"
import "./style.css"
import App from "./App.vue"
import { router } from "./router"
import { i18n } from "./i18n"

createApp(App).use(router).use(i18n).use(ui).use(createHead()).mount("#app")
