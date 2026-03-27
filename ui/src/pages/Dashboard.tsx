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
        <div className="border border-error/50 bg-error/10 p-6 text-center">
          <p className="text-error font-medium">Connection Error</p>
          <p className="text-sm text-error/70 mt-1">{error}</p>
          <button
            onClick={() => {
              setError(null);
              loadData();
            }}
            className="mt-4 px-4 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // Dashboard view
  return (
    <div>
      {/* Header */}
      <div className="px-8 pt-6 pb-0">
        <h2 className="font-medium uppercase text-cream text-[32px] leading-[1.4]">
          Dashboard
        </h2>
        <p className="text-cream-muted text-base mt-2">Your memory at a glance.</p>
      </div>

      {/* Status cards */}
      <div className="grid grid-cols-3 gap-6 px-8 mt-6">
        <StatusCard
          label="Indexed chunks"
          value={status?.indexed_chunks ?? "—"}
        />
        <StatusCard label="Sessions" value={sessions.length} />
        <StatusCard
          label="Embedding model"
          value={status?.embedding_model ?? "—"}
          detail={status ? `${status.embedding_dimensions}d` : undefined}
        />
      </div>

      {/* Search */}
      <div className="px-8 mt-10">
        <h3 className="text-lg font-semibold text-cream mb-4">
          Search memories
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
        <div className="px-8 mt-4">
          <button
            onClick={() => {
              setSearchResults(null);
              setSearchQuery("");
            }}
            className="text-xs text-cream-dim hover:text-cream-muted mb-2 transition-colors"
          >
            Clear results
          </button>
          {searchResults.length === 0 ? (
            <p className="text-sm text-cream-muted">No results found.</p>
          ) : (
            <div className="space-y-3">
              {searchResults.map((r) => (
                <div
                  key={r.chunk_id}
                  className="border border-border bg-bg p-6"
                >
                  <div className="flex items-center gap-2 text-sm text-cream-dim">
                    <span className="font-mono">{r.score.toFixed(3)}</span>
                    <span>&bull;</span>
                    <span>{r.session_id}</span>
                    <span>&bull;</span>
                    <span>{r.chunk_type}</span>
                    {r.role && (
                      <>
                        <span>&bull;</span>
                        <span>{r.role}</span>
                      </>
                    )}
                  </div>
                  <p className="text-sm text-cream-muted mt-2 line-clamp-3">
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
        <div className="px-8 mt-10 pb-8">
          <h3 className="text-lg font-semibold text-cream mb-4">
            Recent sessions
          </h3>
          <SessionList sessions={sessions} onSelect={openSession} />
        </div>
      )}
    </div>
  );
}
