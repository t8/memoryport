import { Outlet, NavLink } from "react-router-dom";
import { Database, Search, Settings, LayoutDashboard } from "lucide-react";

const navItems = [
  { to: "/", icon: LayoutDashboard, label: "Dashboard" },
];

export default function Layout() {
  return (
    <div className="flex h-screen">
      {/* Sidebar */}
      <nav className="w-56 border-r border-zinc-800 bg-zinc-950 flex flex-col">
        <div className="p-4 border-b border-zinc-800">
          <h1 className="text-lg font-bold tracking-tight">Memoryport</h1>
          <p className="text-xs text-zinc-500">Destroyer of the context window</p>
        </div>
        <div className="flex-1 p-2 space-y-1">
          {navItems.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) =>
                `flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors ${
                  isActive
                    ? "bg-zinc-800 text-zinc-100"
                    : "text-zinc-400 hover:text-zinc-200 hover:bg-zinc-900"
                }`
              }
            >
              <item.icon size={16} />
              {item.label}
            </NavLink>
          ))}
        </div>
        <div className="p-4 border-t border-zinc-800 text-xs text-zinc-600">
          v0.1.0
        </div>
      </nav>

      {/* Main content */}
      <main className="flex-1 overflow-auto bg-zinc-950">
        <Outlet />
      </main>
    </div>
  );
}
