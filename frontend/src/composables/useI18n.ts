import { computed } from "vue"
import { useI18n as useVueI18n } from "vue-i18n"
import { useHead, useSeoMeta } from "@unhead/vue"
import type { LangKey } from "../types/api"
import { LANG_LOCALE, OG_LOCALE } from "../i18n"

export function useI18n() {
  const { t, locale, n, d } = useVueI18n({ useScope: "global" })

  function number(value: number): string {
    return n(value || 0)
  }

  function setLang(l: LangKey) {
    locale.value = l
  }

  return { t, lang: locale, number, d, setLang }
}

/** Called once in App.vue: SEO meta and <html lang> update reactively with locale. */
export function useSeoHead() {
  const { t, locale } = useVueI18n({ useScope: "global" })
  const lang = computed(() => locale.value as LangKey)

  useHead({ htmlAttrs: { lang: () => LANG_LOCALE[lang.value] } })
  useSeoMeta({
    title: () => t("seo_title"),
    description: () => t("seo_desc"),
    ogTitle: () => t("seo_title"),
    ogDescription: () => t("seo_desc"),
    ogLocale: () => OG_LOCALE[lang.value],
    twitterTitle: () => t("seo_title"),
    twitterDescription: () => t("seo_desc"),
  })
}
