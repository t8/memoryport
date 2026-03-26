import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  getStatus,
  listSessions,
  type Status,
  type SessionInfo,
  type SearchResult,
} from "../lib/api";
import StatusCard from "../components/StatusCard";
import SessionList from "../components/SessionList";
import SearchBar from "../components/SearchBar";
import Highlight from "../components/Highlight";

export default function Dashboard() {
  const navigate = useNavigate();
  const [status, setStatus] = useState<Status | null>(null);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [searchResults, setSearchResults] = useState<SearchResult[] | null>(
    null
  );
  const [searchQuery, setSearchQuery] = useState("");
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

  function openSession(sessionId: string) {
    navigate(`/session/${encodeURIComponent(sessionId)}`);
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
