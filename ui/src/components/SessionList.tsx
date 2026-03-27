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
    <div className="space-y-2">
      {sorted.map((s) => (
        <button
          key={s.session_id}
          onClick={() => onSelect?.(s.session_id)}
          className="w-full text-left px-4 py-3 border border-border hover:border-border-hover bg-bg hover:bg-surface cursor-pointer transition-all group"
        >
          <div className="flex items-center justify-between">
            <span className="text-sm font-semibold text-cream group-hover:text-cream">
              {s.session_id}
            </span>
            <div className="flex items-center gap-2">
              <span className="text-xs text-cream-dim font-mono">
                {s.chunk_count} chunks
              </span>
              <ChevronRight size={14} className="text-cream-dim group-hover:text-cream-muted transition-colors" />
            </div>
          </div>
          <div className="text-xs text-cream-muted mt-0.5">
            {formatDate(s.first_timestamp)}
            {s.first_timestamp !== s.last_timestamp &&
              ` — ${formatDate(s.last_timestamp)}`}
          </div>
        </button>
      ))}
    </div>
  );
}
