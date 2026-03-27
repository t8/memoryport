import { useEffect, useState, useRef, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { Loader2 } from "lucide-react";
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
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [showErrorDetails, setShowErrorDetails] = useState(false);
  const [retryCount, setRetryCount] = useState(0);
  const [isRetrying, setIsRetrying] = useState(false);
  const retryTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const stopAutoRetry = useCallback(() => {
    if (retryTimerRef.current) {
      clearInterval(retryTimerRef.current);
      retryTimerRef.current = null;
    }
    setIsRetrying(false);
  }, []);

  const loadData = useCallback(async () => {
    try {
      const [s, sess] = await Promise.all([getStatus(), listSessions()]);
      setStatus(s);
      setSessions(sess.sessions);
      setError(null);
      setErrorDetails(null);
      setRetryCount(0);
      stopAutoRetry();
    } catch (err) {
      const rawMsg = err instanceof Error ? err.message : String(err);
      setError("connecting");
      setErrorDetails(rawMsg);
    }
  }, [stopAutoRetry]);

  useEffect(() => {
    loadData();
    // Auto-refresh every 15 seconds to pick up new sessions from the proxy
    const refreshInterval = setInterval(loadData, 15000);
    return () => {
      stopAutoRetry();
      clearInterval(refreshInterval);
    };
  }, [loadData, stopAutoRetry]);

  // Start auto-retry when error is set
  useEffect(() => {
    if (error && !retryTimerRef.current) {
      setIsRetrying(true);
      retryTimerRef.current = setInterval(() => {
        setRetryCount((c) => c + 1);
        loadData();
      }, 3000);
    }
    return () => {
      if (!error) stopAutoRetry();
    };
  }, [error, loadData, stopAutoRetry]);

  function openSession(sessionId: string) {
    navigate(`/session/${encodeURIComponent(sessionId)}`);
  }

  if (error) {
    return (
      <div className="p-8">
        <div className="border border-error/50 bg-error/10 p-6 text-center">
          <div className="flex items-center justify-center gap-2 mb-2">
            {isRetrying && <Loader2 size={16} className="animate-spin text-cream-muted" />}
            <p className="text-cream font-medium">Memoryport server is starting...</p>
          </div>
          <p className="text-sm text-cream-muted">
            {retryCount === 0
              ? "Connecting to the Memoryport server..."
              : `Retrying... (attempt ${retryCount})`}
          </p>
          {retryCount >= 3 && (
            <p className="text-xs text-cream-dim mt-2">
              Make sure the server is running. If using Tauri, it should start automatically.
              For web mode, run <code className="font-mono bg-bg/50 px-1">uc-server</code> first.
            </p>
          )}
          {errorDetails && (
            <div className="mt-3">
              <button
                onClick={() => setShowErrorDetails(!showErrorDetails)}
                className="text-xs text-cream-dim hover:text-cream-muted transition-colors"
              >
                {showErrorDetails ? "Hide" : "Show"} technical details
              </button>
              {showErrorDetails && (
                <pre className="mt-1 text-xs text-cream-dim bg-bg/50 p-2 overflow-x-auto font-mono text-left mx-auto max-w-md">
                  {errorDetails}
                </pre>
              )}
            </div>
          )}
          <button
            onClick={() => {
              setRetryCount(0);
              loadData();
            }}
            className="mt-4 px-4 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors"
          >
            Retry now
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
      <div className="grid grid-cols-4 gap-6 px-8 mt-6">
        <StatusCard
          label="Context space"
          value={status ? `${Math.round(status.indexed_chunks * 375 / 1000).toLocaleString()}K` : "—"}
          suffix="tokens"
          tooltip="The total size of your persistent memory — all tokens stored and available for retrieval. This is separate from your model's context window."
        />
        <StatusCard
          label="Indexed chunks"
          value={status?.indexed_chunks?.toLocaleString() ?? "—"}
          tooltip="Individual pieces of text stored in the vector database. Each chunk is ~375 tokens (~1,500 characters)."
        />
        <StatusCard
          label="Sessions"
          value={sessions.length}
          tooltip="Distinct conversation sessions captured by Memoryport. A new session is created after 30 minutes of inactivity."
        />
        <StatusCard
          label="Embedding model"
          value={status?.embedding_model ?? "—"}
          detail={status ? `${status.embedding_dimensions}d` : undefined}
          tooltip="The model used to convert text into vectors for semantic search. Dimensions (d) affect search precision."
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
