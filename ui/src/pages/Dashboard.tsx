import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Loader2 } from "lucide-react";
import { type SearchResult } from "../lib/api";
import { useData } from "../lib/DataContext";
import StatusCard from "../components/StatusCard";
import SessionList from "../components/SessionList";
import SearchBar from "../components/SearchBar";
import Highlight from "../components/Highlight";

export default function Dashboard() {
  const navigate = useNavigate();
  const { status, sessions, error: dataError, refresh } = useData();
  const [searchResults, setSearchResults] = useState<SearchResult[] | null>(
    null
  );
  const [searchQuery, setSearchQuery] = useState("");

  function openSession(sessionId: string) {
    navigate(`/session/${encodeURIComponent(sessionId)}`);
  }

  if (dataError && !status) {
    return (
      <div className="p-8">
        <div className="border border-error/50 bg-error/10 p-6 text-center">
          <div className="flex items-center justify-center gap-2 mb-2">
            <Loader2 size={16} className="animate-spin text-cream-muted" />
            <p className="text-cream font-medium">Memoryport server is starting...</p>
          </div>
          <p className="text-sm text-cream-muted">Connecting...</p>
          <button
            onClick={refresh}
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
          value={status ? Math.round(status.indexed_chunks * 375) : "—"}
          suffix="tokens"
          tooltip="The total size of your persistent memory — all tokens stored and available for retrieval. This is separate from your model's context window."
        />
        <StatusCard
          label="Indexed chunks"
          value={status?.indexed_chunks ?? "—"}
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
          tooltipAlign="right"
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
          {!status ? (
            <div className="flex items-center gap-2 text-cream-muted py-8 justify-center">
              <Loader2 size={16} className="animate-spin" /> Loading sessions...
            </div>
          ) : (
            <SessionList sessions={sessions} onSelect={openSession} />
          )}
        </div>
      )}
    </div>
  );
}
