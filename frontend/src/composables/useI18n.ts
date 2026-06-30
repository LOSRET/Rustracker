import { ref, computed, type Ref, type ComputedRef } from "vue";
import type { LangKey } from "../types/api";
import {
  translations,
  LANG_LOCALE,
  OG_LOCALE,
  NUM_LOCALE,
  detectLang,
  type Translation,
} from "../i18n";

const lang = ref<LangKey>(detectLang());

const t = computed(() => translations[lang.value]);

function number(value: number): string {
  return new Intl.NumberFormat(NUM_LOCALE[lang.value]).format(value || 0);
}

function localeFor(): string {
  return NUM_LOCALE[lang.value];
}

function setLang(l: LangKey) {
  lang.value = l;
  document.documentElement.lang = LANG_LOCALE[l];
  const tr = translations[l];
  document.title = tr.seo_title;
  const setMeta = (sel: string, attr: string, val: string) => {
    const el = document.querySelector(sel);
    if (el) el.setAttribute(attr, val);
  };
  setMeta("meta[name='description']", "content", tr.seo_desc);
  setMeta("meta[property='og:title']", "content", tr.seo_title);
  setMeta("meta[property='og:description']", "content", tr.seo_desc);
  setMeta("meta[property='og:locale']", "content", OG_LOCALE[l]);
  setMeta("meta[name='twitter:title']", "content", tr.seo_title);
  setMeta("meta[name='twitter:description']", "content", tr.seo_desc);
}

export function useI18n(): {
  lang: Ref<LangKey>;
  t: ComputedRef<Translation>;
  number: (value: number) => string;
  localeFor: () => string;
  setLang: (lang: LangKey) => void;
} {
  return { lang, t, number, localeFor, setLang };
}
