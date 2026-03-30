import { useEffect, useState, useMemo } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { getSession, type SessionChunk } from "../lib/api";
import { ChevronLeft, User, Bot, Search, X, Link, Check } from "lucide-react";

function HighlightText({ text, query }: { text: string; query: string }) {
  if (!query.trim()) return <>{text}</>;
  const regex = new RegExp(`(${query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
  const parts = text.split(regex);
  return (
    <>
      {parts.map((part, i) =>
        regex.test(part) ? (
          <mark key={i} className="bg-accent/30 text-accent rounded-sm px-0.5">{part}</mark>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
}

export default function SessionDetail() {
  const { sessionId } = useParams<{ sessionId: string }>();
  const navigate = useNavigate();
  const [chunks, setChunks] = useState<SessionChunk[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

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

  // Filter chunks by search query (case-insensitive)
  const filteredChunks = useMemo(() => {
    if (!searchQuery.trim()) return chunks;
    const q = searchQuery.toLowerCase();
    return chunks.filter((c) => c.content.toLowerCase().includes(q));
  }, [chunks, searchQuery]);

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
          {loading ? "Loading..." : searchQuery ? `${filteredChunks.length} of ${chunks.length} messages` : `${chunks.length} messages`}
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
            placeholder="Search your messages..."
            className="w-full h-12 pl-10 pr-10 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover transition-colors"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery("")}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-cream-dim hover:text-cream transition-colors"
            >
              <X size={16} />
            </button>
          )}
        </div>
      </div>

      {/* Messages */}
      <div className="px-8 mt-6 pb-8">
        {error && (
          <div className="border border-error/50 bg-error/10 p-4 mb-4">
            <p className="text-error text-sm font-medium">Could not load session</p>
            <p className="text-xs text-cream-muted mt-1">
              The session data could not be retrieved. The server may be restarting or the session may no longer exist.
            </p>
            <details className="mt-2">
              <summary className="text-xs text-cream-dim hover:text-cream-muted cursor-pointer transition-colors">
                Technical details
              </summary>
              <pre className="mt-1 text-xs text-cream-dim bg-bg/50 p-2 overflow-x-auto font-mono">
                {error}
              </pre>
            </details>
            <div className="flex items-center gap-3 mt-3">
              <button
                onClick={() => sessionId && loadSession(sessionId)}
                className="px-3 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors"
              >
                Retry
              </button>
              <button
                onClick={() => navigate("/")}
                className="px-3 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors"
              >
                Back to Dashboard
              </button>
            </div>
          </div>
        )}

        {loading ? (
          <div className="flex items-center gap-2 text-cream-muted py-8">
            <div className="w-4 h-4 border-2 border-cream-dim border-t-cream rounded-full animate-spin" />
            Loading session...
          </div>
        ) : filteredChunks.length === 0 && !error ? (
          <p className="text-cream-muted text-sm py-8">
            {searchQuery ? "No messages match your search." : "No chunks found in this session."}
          </p>
        ) : (
          <div className="space-y-3">
            {filteredChunks.map((chunk, i) => (
              <div
                key={chunk.chunk_id || i}
                className="border border-border bg-bg p-6 relative group"
              >
                {chunk.chunk_id && <CopyRefButton chunkId={chunk.chunk_id} />}
                <div className="flex items-center gap-2 mb-4">
                  {chunk.role === "user" ? (
                    <User size={20} className="text-cream-muted" />
                  ) : (
                    <Bot size={20} className="text-accent" />
                  )}
                  <span className="text-sm text-cream">
                    {chunk.role === "user"
                      ? "You"
                      : chunk.source_model || chunk.role || "Assistant"}
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
                  <HighlightText text={chunk.content} query={searchQuery} />
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function CopyRefButton({ chunkId }: { chunkId: string }) {
  const [copied, setCopied] = useState(false);

  return (
    <button
      onClick={async () => {
        await navigator.clipboard.writeText(`memoryport://chunk/${chunkId}`);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      }}
      className="absolute top-4 right-4 opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1.5 text-cream-dim hover:text-cream text-[11px] font-mono px-2 py-1 rounded bg-surface/80 border border-border/50 hover:border-border transition-colors"
    >
      {copied ? (
        <><Check size={12} className="text-accent" /> Copied</>
      ) : (
        <><Link size={12} /> Share with AI</>
      )}
    </button>
  );
}
