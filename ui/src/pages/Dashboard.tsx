import { useEffect, useState } from "react";
import {
  getStatus,
  listSessions,
  getSession,
  type Status,
  type SessionInfo,
  type SessionChunk,
  type SearchResult,
} from "../lib/api";
import StatusCard from "../components/StatusCard";
import SessionList from "../components/SessionList";
import SearchBar from "../components/SearchBar";
import Highlight from "../components/Highlight";
import { ArrowLeft, MessageSquare, Bot, User } from "lucide-react";

export default function Dashboard() {
  const [status, setStatus] = useState<Status | null>(null);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [searchResults, setSearchResults] = useState<SearchResult[] | null>(
    null
  );
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [sessionChunks, setSessionChunks] = useState<SessionChunk[]>([]);
  const [loadingSession, setLoadingSession] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadData();
  }, []);

  async function loadData() {
    try {
      const [s, sess] = await Promise.all([getStatus(), listSessions()]);
      setStatus(s);
      setSessions(sess.sessions);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to connect");
    }
  }

  async function openSession(sessionId: string) {
    setLoadingSession(true);
    setSelectedSession(sessionId);
    try {
      const data = await getSession(sessionId);
      setSessionChunks(data.chunks);
    } catch (err) {
      console.error("Failed to load session:", err);
      setSessionChunks([]);
    } finally {
      setLoadingSession(false);
    }
  }

  function closeSession() {
    setSelectedSession(null);
    setSessionChunks([]);
  }

  if (error) {
    return (
      <div className="p-8">
        <div className="rounded-lg border border-red-900/50 bg-red-950/20 p-6 text-center">
          <p className="text-red-400 font-medium">Connection Error</p>
          <p className="text-sm text-red-400/70 mt-1">{error}</p>
          <button
            onClick={() => {
              setError(null);
              loadData();
            }}
            className="mt-4 px-4 py-1.5 bg-zinc-800 hover:bg-zinc-700 rounded text-sm transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // Session detail view
  if (selectedSession) {
    return (
      <div className="p-8 max-w-4xl">
        <button
          onClick={closeSession}
          className="flex items-center gap-1.5 text-sm text-zinc-400 hover:text-zinc-200 transition-colors mb-6"
        >
          <ArrowLeft size={14} />
          Back to Dashboard
        </button>

        <div className="flex items-center gap-2 mb-1">
          <MessageSquare size={18} className="text-zinc-500" />
          <h2 className="text-xl font-bold tracking-tight">
            Session: {selectedSession}
          </h2>
        </div>
        <p className="text-zinc-500 text-sm mb-6">
          {sessionChunks.length} chunks
        </p>

        {loadingSession ? (
          <div className="flex items-center gap-2 text-zinc-500 py-8">
            <div className="w-4 h-4 border-2 border-zinc-600 border-t-zinc-300 rounded-full animate-spin" />
            Loading session...
          </div>
        ) : sessionChunks.length === 0 ? (
          <p className="text-zinc-500 text-sm py-8">
            No chunks found in this session.
          </p>
        ) : (
          <div className="space-y-3">
            {sessionChunks.map((chunk, i) => (
              <div
                key={chunk.chunk_id || i}
                className={`rounded-lg border p-4 ${
                  chunk.role === "assistant"
                    ? "border-zinc-700/50 bg-zinc-900/30"
                    : "border-zinc-800 bg-zinc-900/60"
                }`}
              >
                <div className="flex items-center gap-2 mb-2">
                  {chunk.role === "assistant" ? (
                    <Bot size={14} className="text-blue-400" />
                  ) : (
                    <User size={14} className="text-emerald-400" />
                  )}
                  <span
                    className={`text-xs font-medium ${
                      chunk.role === "assistant"
                        ? "text-blue-400"
                        : "text-emerald-400"
                    }`}
                  >
                    {chunk.role === "assistant" && chunk.source_model
                      ? chunk.source_model
                      : chunk.role || "unknown"}
                  </span>
                  {chunk.source_integration && (
                    <span className="text-xs text-zinc-600 bg-zinc-800 px-1.5 py-0.5 rounded">
                      {chunk.source_integration}
                    </span>
                  )}
                  <span className="text-xs text-zinc-600">
                    {new Date(chunk.timestamp).toLocaleString()}
                  </span>
                </div>
                <p className="text-sm text-zinc-300 whitespace-pre-wrap leading-relaxed">
                  {chunk.content}
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  // Dashboard view
  return (
    <div className="p-8 max-w-5xl">
      <h2 className="text-2xl font-bold tracking-tight">Dashboard</h2>
      <p className="text-zinc-500 text-sm mt-1">Your memory at a glance</p>

      {/* Status cards */}
      <div className="grid grid-cols-3 gap-4 mt-6">
        <StatusCard
          label="Indexed Chunks"
          value={status?.indexed_chunks ?? "—"}
        />
        <StatusCard label="Sessions" value={sessions.length} />
        <StatusCard
          label="Embedding Model"
          value={status?.embedding_model ?? "—"}
          detail={status ? `${status.embedding_dimensions}d` : undefined}
        />
      </div>

      {/* Search */}
      <div className="mt-8">
        <h3 className="text-sm font-medium text-zinc-400 mb-2">
          Search Memory
        </h3>
        <SearchBar
          onResults={(results, query) => {
            setSearchResults(results);
            setSearchQuery(query);
          }}
        />
      </div>

      {/* Search results */}
      {searchResults && (
        <div className="mt-4">
          <button
            onClick={() => {
              setSearchResults(null);
              setSearchQuery("");
            }}
            className="text-xs text-zinc-500 hover:text-zinc-300 mb-2 transition-colors"
          >
            Clear results
          </button>
          {searchResults.length === 0 ? (
            <p className="text-sm text-zinc-500">No results found.</p>
          ) : (
            <div className="space-y-2">
              {searchResults.map((r) => (
                <div
                  key={r.chunk_id}
                  className="rounded-md border border-zinc-800 bg-zinc-900/30 p-3"
                >
                  <div className="flex items-center gap-2 text-xs text-zinc-500">
                    <span className="font-mono">{r.score.toFixed(3)}</span>
                    <span>·</span>
                    <span>{r.session_id}</span>
                    <span>·</span>
                    <span>{r.chunk_type}</span>
                    {r.role && (
                      <>
                        <span>·</span>
                        <span>{r.role}</span>
                      </>
                    )}
                  </div>
                  <p className="text-sm text-zinc-300 mt-1 line-clamp-3">
                    <Highlight text={r.content} query={searchQuery} />
                  </p>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Sessions */}
      {!searchResults && (
        <div className="mt-8">
          <h3 className="text-sm font-medium text-zinc-400 mb-2">
            Recent Sessions
          </h3>
          <SessionList sessions={sessions} onSelect={openSession} />
        </div>
      )}
    </div>
  );
}
