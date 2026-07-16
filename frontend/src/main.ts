import { createApp } from "vue"
import ui from "@nuxt/ui/vue-plugin"
import "./style.css"
import App from "./App.vue"
import { i18n } from "./i18n"

createApp(App).use(i18n).use(ui).mount("#app")
