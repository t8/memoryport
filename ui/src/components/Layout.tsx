import { Outlet, NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  BarChart3,
  Network,
  Plug,
  Settings,
  Database,
} from "lucide-react";
import ServiceStatus from "./ServiceStatus";

const navItems = [
  { to: "/", icon: LayoutDashboard, label: "Dashboard" },
  { to: "/analytics", icon: BarChart3, label: "Analytics" },
  { to: "/graph", icon: Network, label: "Graph" },
  { to: "/integrations", icon: Plug, label: "Integrations" },
  { to: "/settings", icon: Settings, label: "Settings" },
];

export default function Layout() {
  return (
    <div className="flex h-screen bg-bg">
      {/* Sidebar */}
      <nav className="w-56 border-r border-border bg-bg flex flex-col">
        <div className="p-4 border-b border-border">
          <div className="flex items-center gap-2">
            <Database size={16} className="text-cream" />
            <h1 className="font-display uppercase text-cream text-sm tracking-wide">Memoryport</h1>
          </div>
        </div>
        <div className="flex-1 py-2">
          {navItems.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) =>
                `flex items-center gap-2.5 px-4 py-2 text-sm transition-colors ${
                  isActive
                    ? "text-cream border-l-2 border-cream bg-surface"
                    : "text-cream-muted hover:text-cream hover:bg-surface border-l-2 border-transparent"
                }`
              }
            >
              <item.icon size={16} />
              {item.label}
            </NavLink>
          ))}
        </div>
        <ServiceStatus />
        <div className="px-4 py-3 border-t border-border text-xs text-cream-dim font-mono">
          v0.1.0
        </div>
      </nav>

      {/* Main content */}
      <main className="flex-1 overflow-auto bg-bg">
        <Outlet />
      </main>
    </div>
  );
}
