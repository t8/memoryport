import { useEffect, useState, lazy, Suspense } from "react";
import { Routes, Route, useNavigate, useLocation } from "react-router-dom";
import { checkConfigExists, isTauri } from "./lib/api";
import { events } from "./lib/telemetry";
import Layout from "./components/Layout";
import Dashboard from "./pages/Dashboard";
import SessionDetail from "./pages/SessionDetail";
import Integrations from "./pages/Integrations";
import Settings from "./pages/Settings";
import Setup from "./pages/Setup";

// Lazy-load expensive pages (d3 charts, graph simulation)
const Analytics = lazy(() => import("./pages/Analytics"));
const Graph = lazy(() => import("./pages/Graph"));

function FirstRunGuard({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();
  const location = useLocation();
  const [checked, setChecked] = useState(false);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [showErrorDetails, setShowErrorDetails] = useState(false);

  useEffect(() => {
    if (location.pathname === "/setup") {
      setChecked(true);
      return;
    }

    // Only check in Tauri mode -- web mode assumes server is running
    if (!isTauri()) {
      setChecked(true);
      return;
    }

    checkConfigExists()
      .then((exists) => {
        if (!exists) {
          navigate("/setup", { replace: true });
        }
        setChecked(true);
      })
      .catch((err) => {
        const rawMsg = err instanceof Error ? err.message : String(err);
        setCheckError("Could not connect to the Memoryport server.");
        setErrorDetails(rawMsg);
      });
  }, []);

  if (checkError) {
    return (
      <div className="min-h-screen bg-bg flex items-center justify-center p-8">
        <div className="max-w-md w-full border border-error/50 bg-error/10 p-6 text-center">
          <p className="text-cream font-medium">Start the Memoryport server first</p>
          <p className="text-sm text-cream-muted mt-2">
            The app needs the Memoryport backend to be running before it can start.
          </p>
          <div className="mt-4 text-left bg-bg/50 p-3 text-xs font-mono text-cream-dim space-y-1">
            <p># Start the server:</p>
            <p>uc-server --config ~/.memoryport/uc.toml</p>
            <p className="mt-2"># Or, if using the Tauri desktop app,</p>
            <p># restart the application.</p>
          </div>
          {errorDetails && (
            <div className="mt-3">
              <button
                onClick={() => setShowErrorDetails(!showErrorDetails)}
                className="text-xs text-cream-dim hover:text-cream-muted transition-colors"
              >
                {showErrorDetails ? "Hide" : "Show"} technical details
              </button>
              {showErrorDetails && (
                <pre className="mt-1 text-xs text-cream-dim bg-bg/50 p-2 overflow-x-auto font-mono text-left">
                  {errorDetails}
                </pre>
              )}
            </div>
          )}
          <button
            onClick={() => {
              setCheckError(null);
              setErrorDetails(null);
              setShowErrorDetails(false);
              setChecked(false);
              // Re-run the check
              checkConfigExists()
                .then((exists) => {
                  if (!exists) {
                    navigate("/setup", { replace: true });
                  }
                  setChecked(true);
                })
                .catch((err2) => {
                  const rawMsg2 = err2 instanceof Error ? err2.message : String(err2);
                  setCheckError("Could not connect to the Memoryport server.");
                  setErrorDetails(rawMsg2);
                });
            }}
            className="mt-4 px-4 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  if (!checked) {
    return (
      <div className="min-h-screen bg-bg flex flex-col items-center justify-center gap-3">
        <div className="w-5 h-5 border-2 border-cream-dim border-t-cream rounded-full animate-spin" />
        <p className="text-cream-muted text-sm">Loading...</p>
      </div>
    );
  }

  return <>{children}</>;
}

export default function App() {
  useEffect(() => { events.appOpened(); }, []);

  return (
    <FirstRunGuard>
      <Routes>
        <Route path="/setup" element={<Setup />} />
        <Route element={<Layout />}>
          <Route path="/" element={<Dashboard />} />
          <Route path="/session/:sessionId" element={<SessionDetail />} />
          <Route path="/analytics" element={<Analytics />} />
          <Route path="/graph" element={<Graph />} />
          <Route path="/integrations" element={<Integrations />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Routes>
    </FirstRunGuard>
  );
}
