export interface StatsResponse {
  peers: number;
  seeders: number;
  leechers: number;
  torrents: number;
  completed: number;
  rps: number;
  version: string;
  uptime_secs: number;
}

export interface TrendPoint {
  timestamp: number;
  torrents: number;
  peers: number;
  seeders: number;
  leechers: number;
}

export interface TrendsResponse {
  history: TrendPoint[];
}

export interface ClientEntry {
  name: string;
  count: number;
}

export interface ClientHistoryPoint {
  timestamp: number;
  counts: Record<string, number>;
}

export interface ClientsResponse {
  timestamp: number;
  tags: string[];
  clients: ClientEntry[];
  history: ClientHistoryPoint[];
}

export interface Top100Entry {
  info_hash: string;
  seeders: number;
  leechers: number;
  peers: number;
  downloaded: number;
}

export interface Top100Response {
  peers: Top100Entry[];
  seeders: Top100Entry[];
  leechers: Top100Entry[];
  downloaded: Top100Entry[];
}

export type SortKey = "peers" | "seeders" | "leechers" | "downloaded";
export type RangeKey = "24h" | "3d" | "7d";
export type LangKey = "zh" | "en" | "ja" | "ru" | "de" | "uk";
export type PageKey = "dashboard" | "top100";
