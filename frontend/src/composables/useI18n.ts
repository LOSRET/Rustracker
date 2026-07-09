import { useI18n as useVueI18n } from "vue-i18n";
import type { LangKey } from "../types/api";
import { LANG_LOCALE, OG_LOCALE } from "../i18n";

export function useI18n() {
  const { t, locale, n, d } = useVueI18n({ useScope: "global" });

  function number(value: number): string {
    return n(value || 0);
  }

  function setLang(l: LangKey) {
    locale.value = l;
    document.documentElement.lang = LANG_LOCALE[l];
    document.title = t("seo_title");
    const setMeta = (sel: string, attr: string, val: string) => {
      const el = document.querySelector(sel);
      if (el) el.setAttribute(attr, val);
    };
    setMeta("meta[name='description']", "content", t("seo_desc"));
    setMeta("meta[property='og:title']", "content", t("seo_title"));
    setMeta("meta[property='og:description']", "content", t("seo_desc"));
    setMeta("meta[property='og:locale']", "content", OG_LOCALE[l]);
    setMeta("meta[name='twitter:title']", "content", t("seo_title"));
    setMeta("meta[name='twitter:description']", "content", t("seo_desc"));
  }

  return { t, lang: locale, number, d, setLang };
}
