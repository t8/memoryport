import { useRef, useMemo, useEffect, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
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
  const listRef = useRef<HTMLDivElement>(null);
  const [scrollEl, setScrollEl] = useState<HTMLElement | null>(null);

  // Find the nearest scrollable ancestor (<main>) for the virtualizer
  useEffect(() => {
    let el = listRef.current?.parentElement;
    while (el) {
      const style = getComputedStyle(el);
      if (style.overflow === "auto" || style.overflow === "scroll" ||
          style.overflowY === "auto" || style.overflowY === "scroll") {
        setScrollEl(el);
        return;
      }
      el = el.parentElement;
    }
  }, []);

  const sorted = useMemo(
    () => [...sessions].sort((a, b) => b.last_timestamp - a.last_timestamp),
    [sessions]
  );

  const virtualizer = useVirtualizer({
    count: sorted.length,
    getScrollElement: () => scrollEl,
    estimateSize: () => 92,
    overscan: 10,
  });

  if (sorted.length === 0) {
    return (
      <div className="text-center text-cream-muted py-8">
        No sessions stored yet. Start a conversation to build your memory.
      </div>
    );
  }

  if (!scrollEl) {
    // Before scroll container found, render nothing (one frame)
    return <div ref={listRef} />;
  }

  // Calculate offset of the list within the scroll container
  const items = virtualizer.getVirtualItems();

  return (
    <div ref={listRef}>
      <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
        {items.map((virtualRow) => {
          const s = sorted[virtualRow.index];
          return (
            <div
              key={s.session_id}
              data-index={virtualRow.index}
              ref={virtualizer.measureElement}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              <button
                onClick={() => onSelect?.(s.session_id)}
                className="w-full text-left p-6 border border-border hover:border-border-hover bg-bg hover:bg-surface cursor-pointer transition-all group mb-3"
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
            </div>
          );
        })}
      </div>
    </div>
  );
}
