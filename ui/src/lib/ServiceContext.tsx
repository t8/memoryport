import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { getServiceHealth, type ServiceHealthResponse } from "./api";

const POLL_INTERVAL = 10_000; // 10 seconds

const defaultHealth: ServiceHealthResponse = {
  engine: { name: "engine", status: "stopped", uptime_secs: null, restart_count: 0, details: null },
  proxy: { name: "proxy", status: "stopped", uptime_secs: null, restart_count: 0, details: null },
  mcp: { name: "mcp", status: "stopped", uptime_secs: null, restart_count: 0, details: null },
  ollama: { name: "ollama", status: "stopped", uptime_secs: null, restart_count: 0, details: null },
};

interface ServiceContextType {
  health: ServiceHealthResponse;
  loading: boolean;
  lastCheckedAt: number | null;
  refresh: () => void;
}

const ServiceContext = createContext<ServiceContextType>({
  health: defaultHealth,
  loading: true,
  lastCheckedAt: null,
  refresh: () => {},
});

export function ServiceProvider({ children }: { children: ReactNode }) {
  const [health, setHealth] = useState<ServiceHealthResponse>(defaultHealth);
  const [loading, setLoading] = useState(true);
  const [lastCheckedAt, setLastCheckedAt] = useState<number | null>(null);

  async function fetchHealth() {
    try {
      const h = await getServiceHealth();
      setHealth(h);
      setLastCheckedAt(Date.now());
    } catch {
      // Leave as default (stopped)
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    fetchHealth();
    const interval = setInterval(fetchHealth, POLL_INTERVAL);
    return () => clearInterval(interval);
  }, []);

  return (
    <ServiceContext.Provider value={{ health, loading, lastCheckedAt, refresh: fetchHealth }}>
      {children}
    </ServiceContext.Provider>
  );
}

export function useServiceHealth() {
  return useContext(ServiceContext);
}
