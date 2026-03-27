import Tooltip from "./Tooltip";

interface StatusCardProps {
  label: string;
  value: string | number;
  suffix?: string;
  detail?: string;
  tooltip?: string;
}

export default function StatusCard({ label, value, suffix, detail, tooltip }: StatusCardProps) {
  const isNumeric = typeof value === "number" || /^[\d,]+[KMBGTkmb%]?$/.test(String(value));
  return (
    <div className="border border-border bg-bg p-6">
      <div className="flex items-center gap-1.5">
        <p className="text-base text-cream">{label}</p>
        {tooltip && <Tooltip content={tooltip} />}
      </div>
      <div className="flex items-baseline gap-2 mt-1">
        <p
          className={`text-cream ${
            isNumeric
              ? "font-display text-[48px] leading-tight"
              : "text-2xl font-semibold leading-snug"
          }`}
        >
          {typeof value === "number" ? value.toLocaleString() : value}
        </p>
        {suffix && <span className="text-cream-muted text-base">{suffix}</span>}
      </div>
      {detail && <p className="text-sm text-cream-dim mt-1">{detail}</p>}
    </div>
  );
}
