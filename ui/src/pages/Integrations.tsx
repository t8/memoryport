import { useEffect, useState } from "react";
import {
  getStatus,
  getIntegrations,
  toggleIntegration,
  type Status,
  type IntegrationsStatus,
} from "../lib/api";
import Toggle from "../components/Toggle";
import Tooltip from "../components/Tooltip";
import {
  CheckCircle2,
  XCircle,
  MinusCircle,
  Server,
  Shield,
  HardDrive,
  Cpu,
  Loader2,
} from "lucide-react";

type IStatus = "operational" | "down" | "unconfigured";

function StatusBadge({ status }: { status: IStatus }) {
  const config = {
    operational: { icon: CheckCircle2, color: "text-accent", label: "Active" },
    down: { icon: XCircle, color: "text-error", label: "Down" },
    unconfigured: { icon: MinusCircle, color: "text-cream-dim", label: "Off" },
  }[status];
  const Icon = config.icon;
  return (
    <span className={`flex items-center gap-1 text-xs font-mono ${config.color}`}>
      <Icon size={12} />
      {config.label}
    </span>
  );
}

export default function Integrations() {
  const [status, setStatus] = useState<Status | null>(null);
  const [integrations, setIntegrations] = useState<IntegrationsStatus | null>(null);
  const [toggling, setToggling] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    getStatus().then(setStatus).catch(console.error);
    getIntegrations().then(setIntegrations).catch(console.error);
  }, []);

  async function handleToggle(integration: string, enabled: boolean) {
    setToggling(integration);
    setMessage(null);
    try {
      const result = await toggleIntegration(integration, enabled);
      setMessage(result.message);
      const updated = await getIntegrations();
      setIntegrations(updated);
    } catch (e: any) {
      setMessage(`Error: ${e.message}`);
    } finally {
      setToggling(null);
    }
  }

  const mcpEnabled = integrations?.mcp.enabled ?? false;
  const proxyEnabled = integrations?.proxy.enabled ?? false;
  const ollamaEnabled = integrations?.ollama.enabled ?? false;
  const arweaveEnabled = integrations?.arweave.enabled ?? false;

  return (
    <div className="p-8 max-w-4xl space-y-8">
      <div>
        <h2 className="font-display uppercase text-cream text-2xl tracking-wide">Integrations</h2>
        <p className="text-cream-muted text-sm mt-1">
          Manage how Memoryport connects to your tools
        </p>
      </div>

      <div className="border border-border bg-surface px-4 py-3 text-sm text-cream-muted">
        You don't need both MCP and API Proxy active. If you're using Claude Code,
        the MCP server alone handles memory. The proxy is for full request/response
        capture or for tools that don't support MCP.
      </div>

      {message && (
        <div className="border border-border bg-surface px-4 py-3 text-sm text-cream-muted">
          {message}
        </div>
      )}

      <div className="space-y-4">
        {/* MCP Server */}
        <div className="border border-border bg-bg p-5">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-3">
              <Server size={18} className="text-cream-dim" />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="font-medium text-sm text-cream">MCP Server</h3>
                  <Tooltip content="The MCP server exposes Memoryport as tools to AI assistants like Claude Code and Cursor. It runs over stdio and lets the AI store and retrieve memories. Best for editor integrations that support MCP natively." />
                </div>
                <p className="text-xs text-cream-dim mt-0.5">
                  Provides memory tools to Claude Code, Cursor, and other MCP-compatible editors
                </p>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <StatusBadge status={mcpEnabled ? "operational" : "unconfigured"} />
              {toggling === "mcp" ? (
                <Loader2 size={18} className="animate-spin text-cream-dim" />
              ) : (
                <Toggle enabled={mcpEnabled} onChange={(v) => handleToggle("mcp", v)} />
              )}
            </div>
          </div>
          {mcpEnabled && (
            <div className="mt-3 pt-3 border-t border-border grid grid-cols-3 gap-4 text-xs">
              <div>
                <span className="text-cream-dim font-mono">Transport</span>
                <p className="text-cream-muted mt-0.5">stdio</p>
              </div>
              <div>
                <span className="text-cream-dim font-mono">Tools</span>
                <p className="text-cream-muted mt-0.5">7 tools, 2 resources</p>
              </div>
              <div>
                <span className="text-cream-dim font-mono">Auto-capture</span>
                <p className="text-cream-muted mt-0.5">Via uc_auto_store</p>
              </div>
            </div>
          )}
        </div>

        {/* API Proxy */}
        <div className="border border-border bg-bg p-5">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-3">
              <Shield size={18} className="text-cream-dim" />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="font-medium text-sm text-cream">API Proxy</h3>
                  <Tooltip content="The proxy sits between your editor and the AI provider (Anthropic/OpenAI). It captures every message in both directions automatically and injects relevant context from your memory. Use this for full conversation capture without relying on the AI to call tools." />
                </div>
                <p className="text-xs text-cream-dim mt-0.5">
                  Transparent capture of all conversations — both your messages and AI responses
                </p>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <StatusBadge status={proxyEnabled ? "operational" : "unconfigured"} />
              {toggling === "proxy" ? (
                <Loader2 size={18} className="animate-spin text-cream-dim" />
              ) : (
                <Toggle enabled={proxyEnabled} onChange={(v) => handleToggle("proxy", v)} />
              )}
            </div>
          </div>
          {proxyEnabled && (
            <div className="mt-3 pt-3 border-t border-border grid grid-cols-3 gap-4 text-xs">
              <div>
                <span className="text-cream-dim font-mono">Listen</span>
                <p className="text-cream-muted mt-0.5">127.0.0.1:9191</p>
              </div>
              <div>
                <span className="text-cream-dim font-mono">Anthropic</span>
                <p className="text-cream-muted mt-0.5">/v1/messages</p>
              </div>
              <div>
                <span className="text-cream-dim font-mono">OpenAI</span>
                <p className="text-cream-muted mt-0.5">/v1/chat/completions</p>
              </div>
            </div>
          )}
        </div>

        {/* Ollama Intercept */}
        <div className="border border-border bg-bg p-5">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-3">
              <Cpu size={18} className="text-cream-dim" />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="font-medium text-sm text-cream">Ollama Auto-Capture</h3>
                  <Tooltip content="Captures conversations with local Ollama models through the proxy. Works with Open WebUI, Continue.dev, terminal (ollama run), and any tool that supports OLLAMA_HOST. Note: the Ollama desktop chat app doesn't support custom endpoints, so conversations there won't be captured — use one of the supported clients instead." />
                </div>
                <p className="text-xs text-cream-dim mt-0.5">
                  {ollamaEnabled
                    ? "Capturing Ollama conversations via Open WebUI, Continue.dev, terminal, and API clients"
                    : "Memory capture for local Ollama models (Open WebUI, Continue.dev, terminal, API)"}
                </p>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <StatusBadge status={ollamaEnabled ? "operational" : "unconfigured"} />
              {toggling === "ollama" ? (
                <Loader2 size={18} className="animate-spin text-cream-dim" />
              ) : (
                <Toggle enabled={ollamaEnabled} onChange={(v) => handleToggle("ollama", v)} />
              )}
            </div>
          </div>
          {ollamaEnabled && (
            <div className="mt-3 pt-3 border-t border-border space-y-3">
              <div className="grid grid-cols-3 gap-4 text-xs">
                <div>
                  <span className="text-cream-dim font-mono">Proxy</span>
                  <p className="text-cream-muted mt-0.5">127.0.0.1:9191</p>
                </div>
                <div>
                  <span className="text-cream-dim font-mono">Ollama</span>
                  <p className="text-cream-muted mt-0.5">127.0.0.1:11434 (unchanged)</p>
                </div>
                <div>
                  <span className="text-cream-dim font-mono">Status</span>
                  <p className="text-accent mt-0.5">Active</p>
                </div>
              </div>
              <div className="bg-surface border border-border px-3 py-2 text-xs text-cream-muted font-mono">
                OLLAMA_HOST=http://127.0.0.1:9191
              </div>
              <p className="text-xs text-cream-dim">
                Set the above in Open WebUI, Continue.dev, or your shell profile to capture Ollama conversations.
                The Ollama desktop chat app doesn't support custom endpoints — use a supported client instead.
              </p>
            </div>
          )}
        </div>

        {/* Arweave Storage */}
        <div className="border border-border bg-bg p-5">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-3">
              <HardDrive size={18} className="text-cream-dim" />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="font-medium text-sm text-cream">Arweave Storage</h3>
                  <Tooltip content="Arweave is a permanent, decentralized storage network. When enabled, your memories are stored on-chain — pay once, stored forever. Without Arweave, memories are stored locally on your machine only. Configure a wallet in Settings to enable." />
                </div>
                <p className="text-xs text-cream-dim mt-0.5">
                  {arweaveEnabled
                    ? "Memories synced to permanent decentralized storage"
                    : "Memories stored locally only — configure a wallet in Settings to enable"}
                </p>
              </div>
            </div>
            <StatusBadge status={arweaveEnabled ? "operational" : "unconfigured"} />
          </div>
          {arweaveEnabled && (
            <div className="mt-3 pt-3 border-t border-border grid grid-cols-3 gap-4 text-xs">
              <div>
                <span className="text-cream-dim font-mono">Gateway</span>
                <p className="text-cream-muted mt-0.5">arweave.net</p>
              </div>
              <div>
                <span className="text-cream-dim font-mono">Wallet</span>
                <p className="text-cream-muted mt-0.5">Configured</p>
              </div>
              <div>
                <span className="text-cream-dim font-mono">Cost</span>
                <p className="text-cream-muted mt-0.5">~$7/GB (pay once)</p>
              </div>
            </div>
          )}
        </div>

        {/* Embedding Provider */}
        <div className="border border-border bg-bg p-5">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-3">
              <Cpu size={18} className="text-cream-dim" />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="font-medium text-sm text-cream">Embedding Provider</h3>
                  <Tooltip content="The embedding provider converts text into vector representations for semantic search. Ollama runs locally (free, private). OpenAI provides higher quality but requires an API key. Change the provider in Settings." />
                </div>
                <p className="text-xs text-cream-dim mt-0.5">
                  {status
                    ? `${status.embedding_model} (${status.embedding_dimensions}d)`
                    : "Loading..."}
                </p>
              </div>
            </div>
            <StatusBadge status={status ? "operational" : "down"} />
          </div>
          <div className="mt-3 pt-3 border-t border-border grid grid-cols-3 gap-4 text-xs">
            <div>
              <span className="text-cream-dim font-mono">Model</span>
              <p className="text-cream-muted mt-0.5">{status?.embedding_model || "—"}</p>
            </div>
            <div>
              <span className="text-cream-dim font-mono">Dimensions</span>
              <p className="text-cream-muted mt-0.5">{status?.embedding_dimensions || "—"}</p>
            </div>
            <div>
              <span className="text-cream-dim font-mono">Indexed</span>
              <p className="text-cream-muted mt-0.5">{status ? `${status.indexed_chunks} chunks` : "—"}</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
