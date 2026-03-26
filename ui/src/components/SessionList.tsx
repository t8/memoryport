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
      <div className="text-center text-zinc-500 py-8">
        No sessions stored yet. Start a conversation to build your memory.
      </div>
    );
  }

  // Sort: most recent session first
  const sorted = [...sessions].sort(
    (a, b) => b.last_timestamp - a.last_timestamp
  );

  return (
    <div className="space-y-1">
      {sorted.map((s) => (
        <button
          key={s.session_id}
          onClick={() => onSelect?.(s.session_id)}
          className="w-full text-left px-3 py-2 rounded-md border border-transparent hover:border-zinc-700 hover:bg-zinc-800/50 cursor-pointer transition-all group"
        >
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium text-zinc-200 group-hover:text-zinc-100">
              {s.session_id}
            </span>
            <span className="text-xs text-zinc-600">
              {s.chunk_count} chunks
            </span>
          </div>
          <div className="text-xs text-zinc-500 mt-0.5">
            {formatDate(s.first_timestamp)}
            {s.first_timestamp !== s.last_timestamp &&
              ` — ${formatDate(s.last_timestamp)}`}
          </div>
        </button>
      ))}
    </div>
  );
}
