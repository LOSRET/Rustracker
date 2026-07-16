import { useIntervalFn } from "@vueuse/core"
import type { StatsResponse } from "../types/api"
import { useApi } from "./useApi"

export function useStats(intervalMs = 5000) {
  const { data: stats, error, lastUpdated, load } = useApi<StatsResponse>("/api/stats")
  const { pause } = useIntervalFn(load, intervalMs, { immediateCallback: true })
  return { stats, error, lastUpdated, stop: pause }
}
