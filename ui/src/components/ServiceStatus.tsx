import { useState, useEffect } from "react";
import { useServiceHealth } from "../lib/ServiceContext";
import { restartServiceByName, restartAllServices, stopServices, type ServiceInfo } from "../lib/api";
import { RotateCw, Clock, StopCircle } from "lucide-react";

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

const SERVICE_PORTS: Record<string, number> = {
  proxy: 9191,
  engine: 8090,
  ollama: 11434,
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
  const isBadState = info.status === "crashed" || info.status === "stopped";
  const port = SERVICE_PORTS[info.name];

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        className={`w-full flex items-center gap-3 px-6 py-1.5 text-sm transition-colors ${
          isBadState
            ? "bg-[rgba(239,68,68,0.06)] hover:bg-[rgba(239,68,68,0.12)]"
            : "hover:bg-surface"
        }`}
      >
        <span className={`w-2 h-2 rounded-full shrink-0 ${STATUS_COLORS[info.status] || "bg-cream-dim"}`} />
        <span className="text-cream-muted capitalize flex-1 text-left flex items-center gap-2">
          {info.name}
          {port && (
            <span className="text-cream-dim text-[10px] font-mono">:{port}</span>
          )}
        </span>
        <span className="text-cream-dim text-xs font-mono">
          {STATUS_LABELS[info.status] || info.status}
        </span>
      </button>
      {expanded && (
        <div className="ml-11 pr-6 py-1 text-xs text-cream-dim space-y-1">
          {info.uptime_secs != null && (
            <p>Uptime: {formatUptime(info.uptime_secs)}</p>
          )}
          <p>Restarts: {info.restart_count}</p>
          {port && <p>Port: {port}</p>}
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

function LastCheckedLabel({ lastCheckedAt }: { lastCheckedAt: number | null }) {
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  if (!lastCheckedAt) return null;

  const agoSecs = Math.max(0, Math.floor((now - lastCheckedAt) / 1000));
  const label = agoSecs < 2 ? "just now" : `${agoSecs}s ago`;

  return (
    <span className="flex items-center gap-1 text-[10px] text-cream-dim font-mono">
      <Clock size={10} className="shrink-0" />
      {label}
    </span>
  );
}

export default function ServiceStatus() {
  const { health, loading, lastCheckedAt, refresh } = useServiceHealth();
  const [restartingAll, setRestartingAll] = useState(false);
  const [stoppingAll, setStoppingAll] = useState(false);

  if (loading) return null;

  const services = [health.engine, health.proxy, health.mcp, health.ollama];

  async function handleRestartAll() {
    setRestartingAll(true);
    try {
      await restartAllServices();
    } catch (e) {
      console.error("Restart all failed:", e);
    }
    setTimeout(() => { refresh(); setRestartingAll(false); }, 1500);
  }

  return (
    <div className="py-3">
      <div className="px-6 flex items-center justify-between mb-2">
        <p className="text-xs text-cream-dim uppercase tracking-wider">
          Services
        </p>
        <LastCheckedLabel lastCheckedAt={lastCheckedAt} />
      </div>
      <div className="space-y-0.5">
        {services.map((s) => (
          <ServiceRow key={s.name} info={s} />
        ))}
      </div>
      <div className="px-6 mt-2 flex items-center gap-3">
        <button
          onClick={handleRestartAll}
          disabled={restartingAll || stoppingAll}
          className="flex items-center gap-1.5 text-[11px] text-cream-dim hover:text-cream transition-colors font-mono"
        >
          <RotateCw size={11} className={restartingAll ? "animate-spin" : ""} />
          {restartingAll ? "Restarting..." : "Restart All"}
        </button>
        <button
          onClick={async () => {
            setStoppingAll(true);
            try {
              await stopServices();
              console.log("stopServices() completed");
            } catch (e) {
              console.error("Stop all failed:", e);
            }
            setTimeout(() => { refresh(); setStoppingAll(false); }, 1500);
          }}
          disabled={stoppingAll || restartingAll}
          className="flex items-center gap-1.5 text-[11px] text-error/70 hover:text-error transition-colors font-mono"
        >
          <StopCircle size={11} />
          {stoppingAll ? "Stopping..." : "Stop All"}
        </button>
      </div>
    </div>
  );
}
