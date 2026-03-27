import { useState } from "react";
import { useServiceHealth } from "../lib/ServiceContext";
import { restartServiceByName, type ServiceInfo } from "../lib/api";
import { RotateCw } from "lucide-react";

const STATUS_COLORS: Record<string, string> = {
  running: "bg-accent",
  starting: "bg-yellow-400",
  unhealthy: "bg-yellow-400",
  crashed: "bg-error",
  stopped: "bg-cream-dim",
};

const STATUS_LABELS: Record<string, string> = {
  running: "Running",
  starting: "Starting",
  unhealthy: "Unhealthy",
  crashed: "Crashed",
  stopped: "Stopped",
};

function ServiceRow({ info }: { info: ServiceInfo }) {
  const [expanded, setExpanded] = useState(false);
  const [restarting, setRestarting] = useState(false);

  async function handleRestart() {
    setRestarting(true);
    try {
      await restartServiceByName(info.name);
    } catch {
      // ignore
    } finally {
      setTimeout(() => setRestarting(false), 3000);
    }
  }

  const canRestart = info.name === "proxy" || info.name === "engine";

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-3 px-6 py-1.5 text-sm hover:bg-surface transition-colors"
      >
        <span className={`w-2 h-2 rounded-full shrink-0 ${STATUS_COLORS[info.status] || "bg-cream-dim"}`} />
        <span className="text-cream-muted capitalize flex-1 text-left">{info.name}</span>
        <span className="text-cream-dim text-xs font-mono">
          {STATUS_LABELS[info.status] || info.status}
        </span>
      </button>
      {expanded && (
        <div className="ml-11 pr-6 py-1 text-xs text-cream-dim space-y-1">
          {info.uptime_secs != null && (
            <p>Uptime: {formatUptime(info.uptime_secs)}</p>
          )}
          {info.restart_count > 0 && (
            <p>Restarts: {info.restart_count}</p>
          )}
          {info.details && <p>{info.details}</p>}
          {canRestart && (
            <button
              onClick={handleRestart}
              disabled={restarting}
              className="flex items-center gap-1.5 text-cream-dim hover:text-cream mt-1"
            >
              <RotateCw size={12} className={restarting ? "animate-spin" : ""} />
              {restarting ? "Restarting..." : "Restart"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function formatUptime(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
}

export default function ServiceStatus() {
  const { health, loading } = useServiceHealth();

  if (loading) return null;

  const services = [health.engine, health.proxy, health.mcp, health.ollama];

  return (
    <div className="py-3">
      <p className="px-6 text-xs text-cream-dim uppercase tracking-wider mb-2">
        Services
      </p>
      <div className="space-y-0.5">
        {services.map((s) => (
          <ServiceRow key={s.name} info={s} />
        ))}
      </div>
    </div>
  );
}
