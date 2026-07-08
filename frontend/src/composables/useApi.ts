import { ref } from "vue";
import { useFetch } from "@vueuse/core";

export function useApi<T>(url: string) {
  const lastUpdated = ref<number | null>(null);
  let prev: T | null = null;

  const { data, error, isFetching, execute } = useFetch(
    url,
    { cache: "no-store" },
    {
      immediate: false,
      updateDataOnError: true,
      afterFetch: (ctx) => {
        prev = ctx.data;
        lastUpdated.value = Date.now();
        return ctx;
      },
      onFetchError: () => ({ data: prev }),
    },
  ).get().json<T>();

  async function load() {
    await execute();
  }

  return { data, loading: isFetching, error, lastUpdated, load };
}
