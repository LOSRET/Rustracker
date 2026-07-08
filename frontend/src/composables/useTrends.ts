import { useFetch, useIntervalFn } from "@vueuse/core";
import type { TrendsResponse, ClientsResponse } from "../types/api";

export function useTrends(intervalMs = 600000) {
  let prevTrends: TrendsResponse | null = null;
  let prevClients: ClientsResponse | null = null;

  const { data: trends, execute: execTrends } = useFetch(
    "/api/trends",
    { cache: "no-store" },
    {
      immediate: false,
      updateDataOnError: true,
      afterFetch: (ctx) => {
        prevTrends = ctx.data;
        return ctx;
      },
      onFetchError: () => ({ data: prevTrends }),
    },
  ).get().json<TrendsResponse>();

  const { data: clients, execute: execClients } = useFetch(
    "/api/clients",
    { cache: "no-store" },
    {
      immediate: false,
      updateDataOnError: true,
      afterFetch: (ctx) => {
        prevClients = ctx.data;
        return ctx;
      },
      onFetchError: () => ({ data: prevClients }),
    },
  ).get().json<ClientsResponse>();

  async function refresh() {
    await Promise.all([execTrends(), execClients()]);
  }

  const { pause, resume } = useIntervalFn(refresh, intervalMs, { immediate: false });

  function start() {
    void refresh();
    resume();
  }

  function stop() {
    pause();
  }

  start();
  return { trends, clients, start, stop };
}
