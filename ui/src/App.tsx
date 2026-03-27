import { useEffect, useState } from "react";
import { Routes, Route, useNavigate, useLocation } from "react-router-dom";
import { checkConfigExists, isTauri } from "./lib/api";
import Layout from "./components/Layout";
import Dashboard from "./pages/Dashboard";
import SessionDetail from "./pages/SessionDetail";
import Analytics from "./pages/Analytics";
import Graph from "./pages/Graph";
import Integrations from "./pages/Integrations";
import Settings from "./pages/Settings";
import Setup from "./pages/Setup";

function FirstRunGuard({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();
  const location = useLocation();
  const [checked, setChecked] = useState(false);

  useEffect(() => {
    if (location.pathname === "/setup") {
      setChecked(true);
      return;
    }

    // Only check in Tauri mode — web mode assumes server is running
    if (!isTauri()) {
      setChecked(true);
      return;
    }

    checkConfigExists().then((exists) => {
      if (!exists) {
        navigate("/setup", { replace: true });
      }
      setChecked(true);
    });
  }, []);

  if (!checked) {
    return (
      <div className="min-h-screen bg-bg flex items-center justify-center text-cream-muted text-sm">
        Loading...
      </div>
    );
  }

  return <>{children}</>;
}

export default function App() {
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
