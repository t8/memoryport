import { ChevronRight } from "lucide-react";
import type { SessionInfo } from "../lib/api";

interface SessionListProps {
  sessions: SessionInfo[];
  onSelect?: (sessionId: string) => void;
}

function formatDate(timestampMs: number): string {
  return new Date(timestampMs).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  }) + " at " + new Date(timestampMs).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export default function SessionList({ sessions, onSelect }: SessionListProps) {
  if (sessions.length === 0) {
    return (
      <div className="text-center text-cream-muted py-8">
        No sessions stored yet. Start a conversation to build your memory.
      </div>
    );
  }

  // Sort: most recent session first
  const sorted = [...sessions].sort(
    (a, b) => b.last_timestamp - a.last_timestamp
  );

  return (
    <div className="space-y-3">
      {sorted.map((s) => (
        <button
          key={s.session_id}
          onClick={() => onSelect?.(s.session_id)}
          className="w-full text-left p-6 border border-border hover:border-border-hover bg-bg hover:bg-surface cursor-pointer transition-all group"
        >
          <div className="flex items-center justify-between">
            <div>
              <p className="text-xl font-semibold text-cream">
                {s.session_id}
              </p>
              <div className="flex items-center gap-2 text-sm text-cream-muted mt-2">
                <span>{formatDate(s.first_timestamp)}</span>
                {s.first_timestamp !== s.last_timestamp && (
                  <>
                    <span>&mdash;</span>
                    <span>{formatDate(s.last_timestamp)}</span>
                  </>
                )}
                <span className="text-cream-dim">&bull;</span>
                <span>
                  {s.chunk_count} {s.chunk_count === 1 ? "chunk" : "chunks"}
                </span>
              </div>
            </div>
            <ChevronRight size={24} className="text-cream-dim group-hover:text-cream-muted transition-colors shrink-0" />
          </div>
        </button>
      ))}
    </div>
  );
}
