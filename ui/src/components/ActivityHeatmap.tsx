interface ActivityHeatmapProps {
  /** Array of { date: "2026-03-25", count: 5 } */
  data: { date: string; count: number }[];
  weeks?: number;
}

const DAYS = ["Mon", "", "Wed", "", "Fri", "", ""];

export default function ActivityHeatmap({
  data,
  weeks = 12,
}: ActivityHeatmapProps) {
  // Build a map for quick lookup
  const countMap = new Map(data.map((d) => [d.date, d.count]));
  const maxCount = Math.max(1, ...data.map((d) => d.count));

  // Generate the grid: weeks × 7 days, ending at today
  const today = new Date();
  const cells: { date: string; count: number; col: number; row: number }[] = [];

  for (let w = weeks - 1; w >= 0; w--) {
    for (let d = 0; d < 7; d++) {
      const date = new Date(today);
      date.setDate(today.getDate() - (w * 7 + (6 - d)));
      const key = date.toISOString().slice(0, 10);
      const count = countMap.get(key) || 0;
      cells.push({
        date: key,
        count,
        col: weeks - 1 - w,
        row: d,
      });
    }
  }

  const cellSize = 12;
  const gap = 2;
  const labelWidth = 24;

  return (
    <div className="overflow-x-auto">
      <svg
        width={labelWidth + weeks * (cellSize + gap)}
        height={7 * (cellSize + gap) + 4}
      >
        {/* Day labels */}
        {DAYS.map(
          (label, i) =>
            label && (
              <text
                key={i}
                x={0}
                y={i * (cellSize + gap) + cellSize - 1}
                className="fill-zinc-600"
                fontSize={9}
              >
                {label}
              </text>
            )
        )}
        {/* Cells */}
        {cells.map((cell, i) => (
          <rect
            key={i}
            x={labelWidth + cell.col * (cellSize + gap)}
            y={cell.row * (cellSize + gap)}
            width={cellSize}
            height={cellSize}
            rx={2}
            fill={
              cell.count === 0
                ? "#27272a" // zinc-800
                : `rgba(16, 185, 129, ${0.2 + (cell.count / maxCount) * 0.8})`
            }
          >
            <title>
              {cell.date}: {cell.count} chunks
            </title>
          </rect>
        ))}
      </svg>
    </div>
  );
}
