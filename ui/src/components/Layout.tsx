import { Outlet, NavLink } from "react-router-dom";
import {
  LayoutAlt02,
  BarChartSquare01,
  Dataflow02,
  Lightning01,
  Settings02,
} from "./NavIcons";
import Logo from "./Logo";
import ServiceStatus from "./ServiceStatus";
import UpdateBadge from "./UpdateBadge";

const navItems = [
  { to: "/", icon: LayoutAlt02, label: "Dashboard" },
  { to: "/analytics", icon: BarChartSquare01, label: "Analytics" },
  { to: "/graph", icon: Dataflow02, label: "Graph" },
  { to: "/integrations", icon: Lightning01, label: "Integrations" },
  { to: "/settings", icon: Settings02, label: "Settings" },
];

export default function Layout() {
  return (
    <div className="flex h-screen bg-bg">
      {/* Sidebar */}
      <nav className="w-[280px] shrink-0 bg-bg p-4">
        <div className="flex flex-col h-full border border-cream">
          {/* Logo */}
          <div className="h-16 flex items-center justify-center border-b border-cream">
            <div className="flex items-center gap-[5px]">
              <Logo className="shrink-0" />
              <h1 className="font-display text-cream text-[29px] leading-none tracking-wide">
                MemoryPort
              </h1>
            </div>
          </div>

          {/* Navigation */}
          <div className="flex-1">
            {navItems.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.to === "/"}
                className={({ isActive }) =>
                  `flex items-center gap-2 h-14 px-6 text-base border-b border-cream transition-colors ${
                    isActive
                      ? "bg-cream text-bg"
                      : "text-cream-muted hover:text-cream"
                  }`
                }
              >
                <item.icon size={20} />
                {item.label}
              </NavLink>
            ))}
          </div>

          {/* Service Status */}
          <ServiceStatus />

          {/* Version */}
          <div className="px-6 py-4 flex items-center gap-2">
            <span className="text-sm text-cream-dim font-mono">v 0.1.1</span>
            <UpdateBadge />
          </div>
        </div>
      </nav>

      {/* Main content */}
      <main className="flex-1 overflow-auto bg-bg">
        <Outlet />
      </main>
    </div>
  );
}
