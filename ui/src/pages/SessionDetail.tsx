import { useEffect, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { getSession, type SessionChunk } from "../lib/api";
import { ArrowLeft, MessageSquare, Bot, User } from "lucide-react";

export default function SessionDetail() {
  const { sessionId } = useParams<{ sessionId: string }>();
  const navigate = useNavigate();
  const [chunks, setChunks] = useState<SessionChunk[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div className="p-8 max-w-4xl">
      <button
        onClick={() => navigate("/")}
        className="flex items-center gap-1.5 text-sm text-zinc-400 hover:text-zinc-200 transition-colors mb-6"
      >
        <ArrowLeft size={14} />
        Back to Dashboard
      </button>

      <div className="flex items-center gap-2 mb-1">
        <MessageSquare size={18} className="text-zinc-500" />
        <h2 className="text-xl font-bold tracking-tight">
          Session: {sessionId}
        </h2>
      </div>
      <p className="text-zinc-500 text-sm mb-6">
        {loading ? "Loading..." : `${chunks.length} messages`}
      </p>

      {error && (
        <div className="rounded-lg border border-red-900/50 bg-red-950/20 p-4 mb-4">
          <p className="text-red-400 text-sm">{error}</p>
          <button
            onClick={() => sessionId && loadSession(sessionId)}
            className="mt-2 text-xs text-zinc-400 hover:text-zinc-200"
          >
            Retry
          </button>
        </div>
      )}

      {loading ? (
        <div className="flex items-center gap-2 text-zinc-500 py-8">
          <div className="w-4 h-4 border-2 border-zinc-600 border-t-zinc-300 rounded-full animate-spin" />
          Loading session...
        </div>
      ) : chunks.length === 0 && !error ? (
        <p className="text-zinc-500 text-sm py-8">
          No chunks found in this session.
        </p>
      ) : (
        <div className="space-y-3">
          {chunks.map((chunk, i) => (
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
