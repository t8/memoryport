import { useEffect, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { getSession, retrieve, type SessionChunk, type SearchResult } from "../lib/api";
import { ChevronLeft, User, Search } from "lucide-react";

export default function SessionDetail() {
  const { sessionId } = useParams<{ sessionId: string }>();
  const navigate = useNavigate();
  const [chunks, setChunks] = useState<SessionChunk[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    if (!sessionId) return;
    loadSession(sessionId);
  }, [sessionId]);

  async function loadSession(id: string) {
    setLoading(true);
    setError(null);
    try {
      const data = await getSession(id);
      const sorted = [...data.chunks].sort((a, b) => {
        if (a.timestamp !== b.timestamp) return a.timestamp - b.timestamp;
        const roleOrder = (r: string | null) =>
          r === "user" ? 0 : r === "assistant" ? 1 : 2;
        return roleOrder(a.role) - roleOrder(b.role);
      });
      setChunks(sorted);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load session");
      setChunks([]);
    } finally {
      setLoading(false);
    }
  }

  async function handleSearch() {
    if (!searchQuery.trim()) return;
    setSearching(true);
    try {
      // Search uses the global retrieve but we filter to this session
      await retrieve(searchQuery, 50);
    } catch {
      // ignore search errors
    } finally {
      setSearching(false);
    }
  }

  return (
    <div>
      {/* Header */}
      <div className="px-8 pt-6">
        <button
          onClick={() => navigate("/")}
          className="flex items-center gap-1 text-sm text-cream-dim hover:text-cream transition-colors mb-2"
        >
          <ChevronLeft size={20} />
          Back to dashboard
        </button>

        <h2 className="font-medium uppercase text-cream text-[32px] leading-[1.4]">
          Session: {sessionId}
        </h2>
        <p className="text-cream-muted text-base mt-2">
          {loading ? "Loading..." : `${chunks.length} messages`}
        </p>
      </div>

      {/* Search */}
      <div className="px-8 mt-6">
        <h3 className="text-lg font-semibold text-cream mb-4">
          Search message
        </h3>
        <div className="relative">
          <Search
            size={18}
            className="absolute left-3 top-1/2 -translate-y-1/2 text-cream-dim"
          />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSearch()}
            placeholder="Search your messages..."
            className="w-full h-12 pl-10 pr-4 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover transition-colors"
          />
          {searching && (
            <div className="absolute right-3 top-1/2 -translate-y-1/2">
              <div className="w-4 h-4 border-2 border-cream-dim border-t-cream rounded-full animate-spin" />
            </div>
          )}
        </div>
      </div>

      {/* Messages */}
      <div className="px-8 mt-6 pb-8">
        {error && (
          <div className="border border-error/50 bg-error/10 p-4 mb-4">
            <p className="text-error text-sm">{error}</p>
            <button
              onClick={() => sessionId && loadSession(sessionId)}
              className="mt-2 text-xs text-cream-dim hover:text-cream"
            >
              Retry
            </button>
          </div>
        )}

        {loading ? (
          <div className="flex items-center gap-2 text-cream-muted py-8">
            <div className="w-4 h-4 border-2 border-cream-dim border-t-cream rounded-full animate-spin" />
            Loading session...
          </div>
        ) : chunks.length === 0 && !error ? (
          <p className="text-cream-muted text-sm py-8">
            No chunks found in this session.
          </p>
        ) : (
          <div className="space-y-3">
            {chunks.map((chunk, i) => (
              <div
                key={chunk.chunk_id || i}
                className="border border-border bg-bg p-6"
              >
                <div className="flex items-center gap-2 mb-4">
                  <User size={20} className="text-cream-muted" />
                  <span className="text-sm text-cream">
                    {chunk.role === "assistant" && chunk.source_model
                      ? chunk.source_model
                      : chunk.role || "unknown"}
                  </span>
                  {chunk.source_integration && (
                    <span className="text-xs text-cream-dim bg-surface border border-border px-2 py-0.5 font-mono rounded">
                      {chunk.source_integration}
                    </span>
                  )}
                  <span className="text-sm text-cream-dim">&bull;</span>
                  <span className="text-sm text-cream-dim">
                    {new Date(chunk.timestamp).toLocaleString()}
                  </span>
                </div>
                <p className="text-sm text-cream-muted whitespace-pre-wrap leading-relaxed">
                  {chunk.content}
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
