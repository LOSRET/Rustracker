import type { ClientListResponse } from "../types/api";
import { useApi } from "./useApi";

export function useClientsList() {
  return useApi<ClientListResponse>("/api/clients/list");
}
