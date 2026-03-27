import { useState } from "react";
import { registerMcp, registerProxy, isTauri } from "../../lib/api";
import { Check, Terminal, Plug } from "lucide-react";

interface Props {
  onNext: () => void;
}

export default function StepIntegrations({ onNext }: Props) {
  const [mcpEnabled, setMcpEnabled] = useState(false);
  const [proxyEnabled, setProxyEnabled] = useState(false);
  const [mcpDone, setMcpDone] = useState(false);
  const [proxyDone, setProxyDone] = useState(false);
  const [loading, setLoading] = useState(false);

  async function handleMcp() {
    setLoading(true);
    try {
      await registerMcp();
      setMcpEnabled(true);
      setMcpDone(true);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  }

  async function handleProxy() {
    setLoading(true);
    try {
      await registerProxy();
      setProxyEnabled(true);
      setProxyDone(true);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-cream">Connect your tools</h2>
        <p className="text-cream-muted text-sm mt-1">
          Enable integrations so your AI tools can use Memoryport.
        </p>
      </div>

      <div className="space-y-3">
        <div className="border border-border p-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Terminal size={20} className="text-cream-dim" />
            <div>
              <p className="text-sm font-medium text-cream">MCP Server</p>
              <p className="text-xs text-cream-dim">Claude Code + Cursor — memory as tools</p>
            </div>
          </div>
          {mcpDone ? (
            <span className="text-accent flex items-center gap-1 text-sm font-mono">
              <Check size={14} /> Registered
            </span>
          ) : (
            <button
              onClick={handleMcp}
              disabled={loading}
              className="px-3 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors disabled:opacity-50"
            >
              Enable
            </button>
          )}
        </div>

        <div className="border border-border p-4 flex items-center justify-between">
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
              disabled={loading}
              className="px-3 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors disabled:opacity-50"
            >
              Enable
            </button>
          )}
        </div>
      </div>

      <p className="text-xs text-cream-dim">
        You can change these later in Settings. Restart Claude Code / Cursor after enabling.
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
