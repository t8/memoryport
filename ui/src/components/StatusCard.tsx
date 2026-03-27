interface StatusCardProps {
  label: string;
  value: string | number;
  detail?: string;
}

export default function StatusCard({ label, value, detail }: StatusCardProps) {
  return (
    <div className="border border-border bg-bg p-4">
      <p className="text-xs text-cream-dim uppercase tracking-wider font-mono">{label}</p>
      <p className="text-2xl font-semibold text-cream mt-1">{value}</p>
      {detail && <p className="text-xs text-cream-dim mt-1">{detail}</p>}
    </div>
  );
}
