interface Segment {
  label: string;
  value: number;
  color: string;
}

interface SegmentBarProps {
  segments: Segment[];
  height?: string;
}

export default function SegmentBar({
  segments,
  height = "h-3",
}: SegmentBarProps) {
  const total = segments.reduce((sum, s) => sum + s.value, 0);
  if (total === 0) {
    return (
      <div className={`w-full ${height} rounded-full bg-zinc-800`} />
    );
  }

  return (
    <div className="space-y-2">
      <div className={`flex w-full overflow-hidden rounded-full bg-zinc-800 ${height}`}>
        {segments.map(
          (s, i) =>
            s.value > 0 && (
              <div
                key={i}
                className="transition-all duration-300"
                style={{
                  width: `${(s.value / total) * 100}%`,
                  backgroundColor: s.color,
                }}
              />
            )
        )}
      </div>
      <div className="flex flex-wrap gap-x-4 gap-y-1">
        {segments.map(
          (s, i) =>
            s.value > 0 && (
              <div key={i} className="flex items-center gap-1.5 text-xs text-zinc-400">
                <span
                  className="inline-block w-2 h-2 rounded-full"
                  style={{ backgroundColor: s.color }}
                />
                {s.label}{" "}
                <span className="text-zinc-500">
                  ({s.value} · {Math.round((s.value / total) * 100)}%)
                </span>
              </div>
            )
        )}
      </div>
    </div>
  );
}
