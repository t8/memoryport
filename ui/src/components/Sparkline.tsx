interface SparklineProps {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
  fillOpacity?: number;
}

export default function Sparkline({
  data,
  width = 200,
  height = 40,
  color = "#84cc16",
  fillOpacity = 0.15,
}: SparklineProps) {
  if (data.length < 2) {
    return (
      <div
        className="flex items-center justify-center text-xs text-cream-dim font-mono"
        style={{ width, height }}
      >
        Not enough data
      </div>
    );
  }

  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const padding = 2;

  const points = data
    .map((v, i) => {
      const x = (i / (data.length - 1)) * (width - padding * 2) + padding;
      const y =
        height - padding - ((v - min) / range) * (height - padding * 2);
      return `${x},${y}`;
    })
    .join(" ");

  // Closed polygon for the fill area
  const fillPoints = `${padding},${height - padding} ${points} ${width - padding},${height - padding}`;

  return (
    <svg width={width} height={height} className="overflow-visible">
      <polygon
        points={fillPoints}
        fill={color}
        opacity={fillOpacity}
      />
      <polyline
        points={points}
        fill="none"
        stroke={color}
        strokeWidth={1.5}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
