import { createContext, useContext, useEffect, useState, useCallback, type ReactNode } from "react";
import {
  getStatus,
  listSessions,
  getAnalytics,
  getGraph,
  getIntegrations,
  type Status,
  type SessionInfo,
  type AnalyticsData,
  type GraphData,
  type IntegrationsStatus,
} from "./api";

const POLL_INTERVAL = 15_000;

interface DataContextType {
  status: Status | null;
  sessions: SessionInfo[];
  analytics: AnalyticsData | null;
  graph: GraphData | null;
  integrations: IntegrationsStatus | null;
  loading: Record<string, boolean>;
  error: string | null;
  fetchAnalytics: () => void;
  fetchGraph: () => void;
  refresh: () => void;
}

const DataContext = createContext<DataContextType>({
  status: null,
  sessions: [],
  analytics: null,
  graph: null,
  integrations: null,
  loading: {},
  error: null,
  fetchAnalytics: () => {},
  fetchGraph: () => {},
  refresh: () => {},
});

export function DataProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [analytics, setAnalytics] = useState<AnalyticsData | null>(null);
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [integrations, setIntegrations] = useState<IntegrationsStatus | null>(null);
  const [loading, setLoading] = useState<Record<string, boolean>>({});
  const [error, setError] = useState<string | null>(null);

  const loadCore = useCallback(async () => {
    try {
      const [s, sess, integ] = await Promise.all([
        getStatus(),
        listSessions(),
        getIntegrations(),
      ]);
      setStatus(s);
      setSessions(sess.sessions);
      setIntegrations(integ);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  // Initial load + polling
  useEffect(() => {
    loadCore();
    const interval = setInterval(loadCore, POLL_INTERVAL);
    return () => clearInterval(interval);
  }, [loadCore]);

  // Lazy-load analytics (expensive)
  const fetchAnalytics = useCallback(async () => {
    if (analytics || loading.analytics) return;
    setLoading((prev) => ({ ...prev, analytics: true }));
    try {
      const data = await getAnalytics();
      setAnalytics(data);
    } catch { /* ignore */ }
    finally { setLoading((prev) => ({ ...prev, analytics: false })); }
  }, [analytics, loading.analytics]);

  // Lazy-load graph (very expensive)
  const fetchGraph = useCallback(async () => {
    if (graph || loading.graph) return;
    setLoading((prev) => ({ ...prev, graph: true }));
    try {
      const data = await getGraph();
      setGraph(data);
    } catch { /* ignore */ }
    finally { setLoading((prev) => ({ ...prev, graph: false })); }
  }, [graph, loading.graph]);

  return (
    <DataContext.Provider value={{
      status,
      sessions,
      analytics,
      graph,
      integrations,
      loading,
      error,
      fetchAnalytics,
      fetchGraph,
      refresh: loadCore,
    }}>
      {children}
    </DataContext.Provider>
  );
}

export function useData() {
  return useContext(DataContext);
}
