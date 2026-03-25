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

export function isTauri(): boolean {
  return IS_TAURI;
}
