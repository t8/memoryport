interface ActivityHeatmapProps {
  /** Array of { date: "2026-03-25", count: 5 } */
  data: { date: string; count: number }[];
  weeks?: number;
}

const DAYS = ["", "Mon", "", "Wed", "", "Fri", ""];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

export default function ActivityHeatmap({
  data,
  weeks = 52,
}: ActivityHeatmapProps) {
  // Build a map for quick lookup
  const countMap = new Map(data.map((d) => [d.date, d.count]));
  const maxCount = Math.max(1, ...data.map((d) => d.count));

  // Generate the grid: weeks x 7 days, ending at today
  const today = new Date();
  const cells: { date: string; count: number; col: number; row: number; month: number }[] = [];

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
        month: date.getMonth(),
      });
    }
  }

  // Compute month labels (first column of each month)
  const monthLabels: { label: string; col: number }[] = [];
  let lastMonth = -1;
  for (const cell of cells) {
    if (cell.row === 0 && cell.month !== lastMonth) {
      monthLabels.push({ label: MONTHS[cell.month], col: cell.col });
      lastMonth = cell.month;
    }
  }

  const cellSize = 12;
  const gap = 2;
  const labelWidth = 28;
  const headerHeight = 18;
  const svgWidth = labelWidth + weeks * (cellSize + gap);
  const svgHeight = headerHeight + 7 * (cellSize + gap) + 4;

  return (
    <div className="overflow-x-auto">
      <svg width={svgWidth} height={svgHeight}>
        {/* Month labels */}
        {monthLabels.map((m, i) => (
          <text
            key={i}
            x={labelWidth + m.col * (cellSize + gap)}
            y={12}
            fill="rgba(255, 244, 224, 0.3)"
            fontSize={10}
            fontFamily="var(--font-mono)"
          >
            {m.label}
          </text>
        ))}
        {/* Day labels */}
        {DAYS.map(
          (label, i) =>
            label && (
              <text
                key={i}
                x={0}
                y={headerHeight + i * (cellSize + gap) + cellSize - 1}
                fill="rgba(255, 244, 224, 0.3)"
                fontSize={9}
                fontFamily="var(--font-mono)"
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
            y={headerHeight + cell.row * (cellSize + gap)}
            width={cellSize}
            height={cellSize}
            rx={2}
            fill={
              cell.count === 0
                ? "#1a1a1a"
                : `rgba(132, 204, 22, ${0.2 + (cell.count / maxCount) * 0.8})`
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
