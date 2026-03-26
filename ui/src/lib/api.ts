// Transport abstraction: Tauri IPC in desktop mode, HTTP fetch in web mode.

const IS_TAURI =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

async function httpGet<T>(path: string): Promise<T> {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);
  return res.json();
}

async function httpPost<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${await res.text()}`);
  return res.json();
}

// ── API Types ──

export interface Status {
  pending_chunks: number;
  indexed_chunks: number;
  index_path: string;
  embedding_model: string;
  embedding_dimensions: number;
}

export interface SessionInfo {
  session_id: string;
  chunk_count: number;
  first_timestamp: number;
  last_timestamp: number;
}

export interface SearchResult {
  chunk_id: string;
  session_id: string;
  chunk_type: string;
  role: string | null;
  score: number;
  timestamp: number;
  content: string;
  arweave_tx_id: string;
}

export interface SessionChunk {
  chunk_id: string;
  role: string | null;
  content: string;
  timestamp: number;
  source_integration: string | null;
  source_model: string | null;
}

// ── API Functions ──

export async function getStatus(): Promise<Status> {
  if (IS_TAURI) return tauriInvoke("get_status");
  return httpGet("/v1/status");
}

export async function listSessions(): Promise<{ sessions: SessionInfo[] }> {
  if (IS_TAURI) {
    const sessions = await tauriInvoke<SessionInfo[]>("list_sessions");
    return { sessions };
  }
  return httpGet("/v1/sessions");
}

export async function getSession(
  sessionId: string
): Promise<{ session_id: string; chunks: SessionChunk[] }> {
  if (IS_TAURI) {
    const chunks = await tauriInvoke<SessionChunk[]>("get_session", {
      sessionId,
    });
    return { session_id: sessionId, chunks };
  }
  return httpGet(`/v1/sessions/${sessionId}`);
}

export async function retrieve(
  query: string,
  topK: number = 10
): Promise<{ results: SearchResult[] }> {
  if (IS_TAURI) {
    const results = await tauriInvoke<SearchResult[]>("retrieve", {
      query,
      topK,
    });
    return { results };
  }
  return httpPost("/v1/retrieve", { query, top_k: topK });
}

// ── Graph ──

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface GraphNode {
  id: string;
  label: string;
  chunk_count: number;
  first_timestamp: number;
  last_timestamp: number;
  x: number;
  y: number;
}

export interface GraphEdge {
  source: string;
  target: string;
  weight: number;
}

export async function getGraph(): Promise<GraphData> {
  if (IS_TAURI) return tauriInvoke("get_graph");
  return httpGet("/v1/graph/sessions");
}

// ── Integrations ──

export interface IntegrationsStatus {
  mcp: { enabled: boolean; status: string };
  proxy: { enabled: boolean; status: string };
  ollama: { enabled: boolean; status: string };
  arweave: { enabled: boolean; status: string };
}

export interface ToggleResponse {
  success: boolean;
  message: string;
}

export async function getIntegrations(): Promise<IntegrationsStatus> {
  if (IS_TAURI) return tauriInvoke("get_integrations");
  return httpGet("/v1/integrations");
}

export async function toggleIntegration(
  integration: string,
  enabled: boolean
): Promise<ToggleResponse> {
  if (IS_TAURI)
    return tauriInvoke("toggle_integration", { integration, enabled });
  return httpPost("/v1/integrations/toggle", { integration, enabled });
}

// ── Analytics ──

export interface AnalyticsData {
  activity: { date: string; count: number }[];
  by_type: Record<string, number>;
  by_source: Record<string, number>;
  by_model: Record<string, number>;
  sync_status: { synced: number; local: number };
  total_chunks: number;
  total_sessions: number;
}

export async function getAnalytics(): Promise<AnalyticsData> {
  if (IS_TAURI) return tauriInvoke("get_analytics");
  return httpGet("/v1/analytics");
}

// ── Settings ──

export interface SettingsData {
  embeddings: {
    provider: string;
    model: string;
    dimensions: number;
    api_key: string | null;
    api_base: string | null;
  };
  retrieval: {
    gating_enabled: boolean;
    similarity_top_k: number;
    recency_window: number;
  };
  arweave: {
    gateway: string;
    wallet_path: string | null;
  };
  encryption: {
    enabled: boolean;
  };
}

export async function getSettings(): Promise<SettingsData> {
  if (IS_TAURI) return tauriInvoke("get_settings");
  return httpGet("/v1/settings");
}

export async function updateSettings(settings: SettingsData): Promise<void> {
  if (IS_TAURI) {
    await tauriInvoke("update_settings", { settings });
    return;
  }
  await httpPost("/v1/settings", settings);
}

export function isTauri(): boolean {
  return IS_TAURI;
}
