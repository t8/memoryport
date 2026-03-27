import { useState } from "react";
import { registerMcp, registerProxy, isTauri } from "../../lib/api";
import { Check, Terminal, Plug, Loader2, AlertTriangle } from "lucide-react";

interface Props {
  onNext: () => void;
}

export default function StepIntegrations({ onNext }: Props) {
  const [mcpEnabled, setMcpEnabled] = useState(false);
  const [proxyEnabled, setProxyEnabled] = useState(false);
  const [mcpDone, setMcpDone] = useState(false);
  const [proxyDone, setProxyDone] = useState(false);
  const [mcpLoading, setMcpLoading] = useState(false);
  const [proxyLoading, setProxyLoading] = useState(false);
  const [mcpError, setMcpError] = useState<string | null>(null);
  const [proxyError, setProxyError] = useState<string | null>(null);

  async function handleMcp() {
    setMcpLoading(true);
    setMcpError(null);
    try {
      await registerMcp();
      setMcpEnabled(true);
      setMcpDone(true);
    } catch (e) {
      setMcpError(e instanceof Error ? e.message : "Failed to register MCP server");
    } finally {
      setMcpLoading(false);
    }
  }

  async function handleProxy() {
    setProxyLoading(true);
    setProxyError(null);
    try {
      await registerProxy();
      setProxyEnabled(true);
      setProxyDone(true);
    } catch (e) {
      setProxyError(e instanceof Error ? e.message : "Failed to configure proxy");
    } finally {
      setProxyLoading(false);
    }
  }

  const anyActivated = mcpDone || proxyDone;

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-cream">Connect your tools</h2>
        <p className="text-cream-muted text-sm mt-1">
          Enable integrations so your AI tools can use Memoryport.
        </p>
      </div>

      <div className="space-y-3">
        {/* MCP Server */}
        <div className="border border-border p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Terminal size={20} className="text-cream-dim" />
              <div>
                <p className="text-sm font-medium text-cream">MCP Server</p>
                <p className="text-xs text-cream-dim">Claude Code + Cursor -- memory as tools</p>
              </div>
            </div>
            {mcpDone ? (
              <span className="text-accent flex items-center gap-1 text-sm font-mono">
                <Check size={14} /> Registered
              </span>
            ) : (
              <button
                onClick={handleMcp}
                disabled={mcpLoading}
                className="px-3 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors disabled:opacity-50 flex items-center gap-1.5"
              >
                {mcpLoading && <Loader2 size={14} className="animate-spin" />}
                {mcpLoading ? "Enabling..." : "Enable"}
              </button>
            )}
          </div>
          {mcpDone && (
            <p className="text-xs text-cream-dim mt-2 ml-8 flex items-center gap-1">
              <AlertTriangle size={12} className="text-cream-dim" />
              Restart Claude Code / Cursor to activate
            </p>
          )}
          {mcpError && (
            <p className="text-xs text-error mt-2 ml-8">{mcpError}</p>
          )}
        </div>

        {/* Proxy */}
        <div className="border border-border p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Plug size={20} className="text-cream-dim" />
              <div>
                <p className="text-sm font-medium text-cream">Auto-Capture Proxy</p>
                <p className="text-xs text-cream-dim">Transparent memory for every conversation</p>
              </div>
            </div>
            {proxyDone ? (
              <span className="text-accent flex items-center gap-1 text-sm font-mono">
                <Check size={14} /> Configured
              </span>
            ) : (
              <button
                onClick={handleProxy}
                disabled={proxyLoading}
                className="px-3 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors disabled:opacity-50 flex items-center gap-1.5"
              >
                {proxyLoading && <Loader2 size={14} className="animate-spin" />}
                {proxyLoading ? "Enabling..." : "Enable"}
              </button>
            )}
          </div>
          {proxyDone && (
            <p className="text-xs text-cream-dim mt-2 ml-8 flex items-center gap-1">
              <AlertTriangle size={12} className="text-cream-dim" />
              Restart Claude Code / Cursor to activate
            </p>
          )}
          {proxyError && (
            <p className="text-xs text-error mt-2 ml-8">{proxyError}</p>
          )}
        </div>
      </div>

      <p className="text-xs text-cream-dim">
        You can change these later in Settings.
        {anyActivated && " Remember to restart your AI tools for changes to take effect."}
      </p>

      <button
        onClick={onNext}
        className="w-full py-2.5 bg-cream text-bg hover:bg-cream/90 text-sm font-medium transition-colors"
      >
        Continue
      </button>
    </div>
  );
}
