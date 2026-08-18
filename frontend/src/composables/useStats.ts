import { useDocumentVisibility, useIntervalFn } from "@vueuse/core"
import { watch } from "vue"
import type { StatsResponse } from "../types/api"
import { useApi } from "./useApi"

export function useStats(intervalMs = 5000) {
  const { data: stats, error, lastUpdated, load } = useApi<StatsResponse>("/api/stats")
  const visibility = useDocumentVisibility()
  const { pause, resume } = useIntervalFn(load, intervalMs, { immediate: false })

  watch(
    visibility,
    (state) => {
      if (state === "visible") {
        void load()
        resume()
      } else {
        pause()
      }
    },
    { immediate: true },
  )

  return { stats, error, lastUpdated, stop: pause }
}
