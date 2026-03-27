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
  operational: { color: "text-accent", icon: CheckCircle2, label: "Operational" },
  degraded: { color: "text-yellow-400", icon: AlertTriangle, label: "Degraded" },
  down: { color: "text-error", icon: XCircle, label: "Down" },
  unconfigured: { color: "text-cream-dim", icon: MinusCircle, label: "Not configured" },
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
    <div className="border border-border bg-bg p-4">
      <div className="flex items-center gap-2 mb-1">
        <Icon size={16} className={config.color} />
        <h3 className="font-medium text-sm text-cream">{name}</h3>
      </div>
      <p className="text-xs text-cream-dim mb-3">{summary}</p>
      <div className="space-y-1.5">
        {details.map((d, i) => (
          <div key={i} className="flex items-center justify-between text-xs">
            <span className="text-cream-dim">{d.label}</span>
            <span
              className={
                d.status
                  ? statusConfig[d.status].color
                  : "text-cream-muted"
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
