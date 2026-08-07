import type { RangeKey } from "../types/api"

const RANGE_SECS: Record<RangeKey, number> = {
  "24h": 86400,
  "3d": 259200,
  "7d": 604800,
}

/** Keep only history points newer than the range window. */
export function filterRange<T extends { timestamp: number }>(items: T[], range: RangeKey): T[] {
  if (!items.length) return items
  const cutoff = Math.floor(Date.now() / 1000) - RANGE_SECS[range]
  return items.filter((item) => item.timestamp >= cutoff)
}

export function emptyChartOption(text: string) {
  return {
    title: {
      text,
      left: "center",
      top: "center",
      textStyle: { color: "#94a3b8", fontSize: 14 },
    },
    series: [],
  }
}

/** Base line series config shared by trend charts. */
export function lineSeries<T extends object>(name: string, data: number[], extra?: T) {
  return { name, type: "line", smooth: true, showSymbol: false, data, ...extra }
}

/** Theme-aware tooltip/legend/grid/axis config shared by trend charts. */
export function baseChart(dark: boolean, legendData: string[], labels: string[]) {
  const cc = dark
    ? { axis: "#94a3b8", line: "#334155", legend: "#cbd5e1" }
    : { axis: "#64748b", line: "#e6ebf2", legend: "#1f2937" }
  return {
    tooltip: {
      trigger: "axis",
      backgroundColor: dark ? "#1e293b" : "#ffffff",
      borderColor: dark ? "#334155" : "#e2e8f0",
      textStyle: { color: dark ? "#e2e8f0" : "#1f2937" },
    },
    legend: {
      type: "scroll",
      top: 0,
      left: "center",
      itemWidth: 16,
      itemGap: 14,
      textStyle: { fontSize: 11, color: cc.legend },
      data: legendData,
    },
    grid: { left: 4, right: 4, top: 52, bottom: 36, containLabel: true },
    xAxis: {
      type: "category",
      boundaryGap: false,
      data: labels,
      axisLine: { lineStyle: { color: cc.line } },
      axisLabel: { color: cc.axis },
    },
    yAxis: {
      type: "value",
      minInterval: 1,
      axisLabel: { color: cc.axis },
      splitLine: { lineStyle: { color: cc.line } },
    },
  }
}
