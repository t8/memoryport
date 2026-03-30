import Tooltip from "./Tooltip";

interface StatusCardProps {
  label: string;
  value: string | number;
  suffix?: string;
  detail?: string;
  tooltip?: string;
  tooltipAlign?: "center" | "left" | "right";
}

function compactNumber(value: string | number): string {
  const num = typeof value === "number" ? value : parseFloat(String(value).replace(/,/g, ""));
  if (isNaN(num)) return String(value);
  if (num >= 1_000_000_000) return `${(num / 1_000_000_000).toFixed(1)}B`;
  if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
  if (num >= 10_000) return `${Math.round(num / 1000)}K`;
  if (num >= 1_000) return `${(num / 1000).toFixed(1)}K`;
  return num.toLocaleString();
}

export default function StatusCard({ label, value, suffix, detail, tooltip, tooltipAlign }: StatusCardProps) {
  const isNumeric = typeof value === "number" || /^[\d,]+[KMBGTkmb%]?$/.test(String(value));
  // If already has a unit suffix (K, M, B), keep as-is; otherwise compact
  const raw = String(value);
  const hasUnit = /[KMBGTkmb]$/.test(raw);
  const displayValue = hasUnit ? raw : (isNumeric ? compactNumber(value) : raw);

  return (
    <div className="border border-border bg-bg p-6">
      <div className="flex items-center gap-1.5">
        <p className="text-base text-cream">{label}</p>
        {tooltip && <Tooltip content={tooltip} align={tooltipAlign} />}
      </div>
      <div className="flex flex-wrap items-baseline gap-x-2 mt-1">
        <p
          className={`text-cream ${
            isNumeric
              ? "font-display text-[48px] leading-tight"
              : "text-2xl font-semibold leading-snug"
          }`}
        >
          {displayValue}
        </p>
        {suffix && <span className="text-cream-muted text-base">{suffix}</span>}
      </div>
      {detail && <p className="text-sm text-cream-dim mt-1">{detail}</p>}
    </div>
  );
}
