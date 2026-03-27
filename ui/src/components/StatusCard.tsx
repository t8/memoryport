interface StatusCardProps {
  label: string;
  value: string | number;
  detail?: string;
}

export default function StatusCard({ label, value, detail }: StatusCardProps) {
  const isNumeric = typeof value === "number" || /^[\d,]+$/.test(String(value));
  return (
    <div className="border border-border bg-bg p-6">
      <p className="text-base text-cream">{label}</p>
      <p
        className={`text-cream mt-1 ${
          isNumeric
            ? "font-display text-[48px] leading-tight"
            : "text-2xl font-semibold leading-snug"
        }`}
      >
        {typeof value === "number" ? value.toLocaleString() : value}
      </p>
      {detail && <p className="text-sm text-cream-dim mt-1">{detail}</p>}
    </div>
  );
}
