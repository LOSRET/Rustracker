import { createApp } from "vue"
import { createHead } from "@unhead/vue/client"
import "virtual:uno.css"
import "./style.css"
import App from "./App.vue"
import { i18n } from "./i18n"

createApp(App).use(i18n).use(createHead()).mount("#app")
