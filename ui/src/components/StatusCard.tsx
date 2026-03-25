interface StatusCardProps {
  label: string;
  value: string | number;
  detail?: string;
}

export default function StatusCard({ label, value, detail }: StatusCardProps) {
  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
      <p className="text-xs text-zinc-500 uppercase tracking-wider">{label}</p>
      <p className="text-2xl font-semibold mt-1">{value}</p>
      {detail && <p className="text-xs text-zinc-500 mt-1">{detail}</p>}
    </div>
  );
}
