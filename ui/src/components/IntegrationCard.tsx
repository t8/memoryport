import { CheckCircle2, AlertTriangle, XCircle, MinusCircle } from "lucide-react";

type Status = "operational" | "degraded" | "down" | "unconfigured";

interface DetailRow {
  label: string;
  value: string;
  status?: Status;
}

interface IntegrationCardProps {
  name: string;
  status: Status;
  summary: string;
  details: DetailRow[];
}

const statusConfig: Record<
  Status,
  { color: string; icon: typeof CheckCircle2; label: string }
> = {
  operational: { color: "text-emerald-400", icon: CheckCircle2, label: "Operational" },
  degraded: { color: "text-amber-400", icon: AlertTriangle, label: "Degraded" },
  down: { color: "text-red-400", icon: XCircle, label: "Down" },
  unconfigured: { color: "text-zinc-500", icon: MinusCircle, label: "Not configured" },
};

export default function IntegrationCard({
  name,
  status,
  summary,
  details,
}: IntegrationCardProps) {
  const config = statusConfig[status];
  const Icon = config.icon;

  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
      <div className="flex items-center gap-2 mb-1">
        <Icon size={16} className={config.color} />
        <h3 className="font-medium text-sm">{name}</h3>
      </div>
      <p className="text-xs text-zinc-500 mb-3">{summary}</p>
      <div className="space-y-1.5">
        {details.map((d, i) => (
          <div key={i} className="flex items-center justify-between text-xs">
            <span className="text-zinc-500">{d.label}</span>
            <span
              className={
                d.status
                  ? statusConfig[d.status].color
                  : "text-zinc-300"
              }
            >
              {d.value}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
