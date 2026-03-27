import { useEffect, useState } from "react";
import {
  getStatus,
  getIntegrations,
  toggleIntegration,
  type Status,
  type IntegrationsStatus,
} from "../lib/api";
import { useServiceHealth } from "../lib/ServiceContext";
import Toggle from "../components/Toggle";
import Tooltip from "../components/Tooltip";
import {
  Server,
  Shield,
  Loader2,
  AlertTriangle,
} from "lucide-react";

function StatusBadge({ status }: { status: "running" | "stopped" | "starting" | "unhealthy" | "crashed" }) {
  const isUp = status === "running";
  const label = isUp ? "Active" : status === "starting" ? "Starting" : "Inactive";
  return (
    <span
      className={`inline-flex items-center gap-1.5 text-xs font-mono px-2 py-0.5 rounded ${
        isUp
          ? "text-accent bg-[rgba(132,204,22,0.1)]"
          : status === "starting"
          ? "text-yellow-400 bg-[rgba(250,204,21,0.1)]"
          : "text-cream-dim bg-[rgba(255,244,224,0.05)]"
      }`}
    >
      <span
        className={`w-1.5 h-1.5 rounded-full ${
          isUp ? "bg-accent" : status === "starting" ? "bg-yellow-400" : "bg-cream-dim"
        }`}
      />
      {label}
    </span>
  );
}

function ServiceOfflineWarning() {
  return (
    <div className="flex items-center gap-2 mt-3 text-xs text-cream-dim">
      <AlertTriangle size={12} className="text-error shrink-0" />
      <span>Service offline</span>
    </div>
  );
}

export default function Integrations() {
  const [status, setStatus] = useState<Status | null>(null);
  const [integrations, setIntegrations] = useState<IntegrationsStatus | null>(null);
  const [toggling, setToggling] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const { health } = useServiceHealth();

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
    <div>
      {/* Header */}
      <div className="px-8 pt-6">
        <h2 className="font-medium uppercase text-cream text-[32px] leading-[1.4]">
          Integrations
        </h2>
        <p className="text-cream-muted text-base mt-2">
          Manage how Memoryport connects to your tools
        </p>
      </div>

      {message && (
        <div className="mx-8 mt-4 border border-border bg-surface px-4 py-3 text-sm text-cream-muted">
          {message}
        </div>
      )}

      <div className="px-8 mt-6 space-y-6 pb-8">
        {/* MCP Server */}
        <div className="border border-border bg-bg p-6">
          <div className="flex items-start justify-between">
            <div className="flex items-start gap-4">
              <Server size={32} className="text-cream-dim mt-1 shrink-0" />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-xl font-semibold text-cream">MCP Server</h3>
                  <StatusBadge status={health.mcp.status} />
                  <Tooltip content="The MCP server exposes Memoryport as tools to AI assistants like Claude Code and Cursor. It runs over stdio and lets the AI store and retrieve memories." />
                </div>
                <p className="text-sm text-cream-muted mt-1">
                  Provides memory tools to Claude Code, Cursor, and other MCP-compatible editors
                </p>
                {health.mcp.status !== "running" && mcpEnabled && <ServiceOfflineWarning />}
              </div>
            </div>
            <div className="shrink-0 ml-4">
              {toggling === "mcp" ? (
                <Loader2 size={20} className="animate-spin text-cream-dim" />
              ) : (
                <Toggle enabled={mcpEnabled} onChange={(v) => handleToggle("mcp", v)} />
              )}
            </div>
          </div>
          {mcpEnabled && (
            <>
              <div className="border-t border-border mt-6 mb-6" />
              <div className="grid grid-cols-3 gap-6">
                <div>
                  <p className="text-sm text-cream-muted">Transport</p>
                  <p className="text-xl font-semibold text-cream mt-1">stdio</p>
                </div>
                <div>
                  <p className="text-sm text-cream-muted">Tools</p>
                  <p className="text-xl font-semibold text-cream mt-1">7 tools, 2 resources</p>
                </div>
                <div>
                  <p className="text-sm text-cream-muted">Auto-capture</p>
                  <p className="text-xl font-semibold text-cream mt-1">Via uc_auto_store</p>
                </div>
              </div>
            </>
          )}
        </div>

        {/* API Proxy */}
        <div className="border border-border bg-bg p-6">
          <div className="flex items-start justify-between">
            <div className="flex items-start gap-4">
              <Shield size={32} className="text-cream-dim mt-1 shrink-0" />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-xl font-semibold text-cream">API Proxy</h3>
                  <StatusBadge status={health.proxy.status} />
                  <Tooltip content="The proxy sits between your editor and the AI provider (Anthropic/OpenAI). It captures every message automatically and injects relevant context from your memory." />
                </div>
                <p className="text-sm text-cream-muted mt-1">
                  Transparent capture of all conversations — both your messages and AI responses
                </p>
                {health.proxy.status !== "running" && proxyEnabled && <ServiceOfflineWarning />}
              </div>
            </div>
            <div className="shrink-0 ml-4">
              {toggling === "proxy" ? (
                <Loader2 size={20} className="animate-spin text-cream-dim" />
              ) : (
                <Toggle enabled={proxyEnabled} onChange={(v) => handleToggle("proxy", v)} />
              )}
            </div>
          </div>
          {proxyEnabled && (
            <>
              <div className="border-t border-border mt-6 mb-6" />
              <div className="grid grid-cols-3 gap-6">
                <div>
                  <p className="text-sm text-cream-muted">Listen</p>
                  <p className="text-xl font-semibold text-cream mt-1">127.0.0.1:9191</p>
                </div>
                <div>
                  <p className="text-sm text-cream-muted">Anthropic</p>
                  <p className="text-xl font-semibold text-cream mt-1">/v1/messages</p>
                </div>
                <div>
                  <p className="text-sm text-cream-muted">OpenAI</p>
                  <p className="text-xl font-semibold text-cream mt-1">/v1/chat/completions</p>
                </div>
              </div>
            </>
          )}
        </div>

        {/* Ollama Auto-Capture */}
        <div className="border border-border bg-bg p-6">
          <div className="flex items-start justify-between">
            <div className="flex items-start gap-4">
              <OllamaIcon />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-xl font-semibold text-cream">Ollama Auto-Capture</h3>
                  <StatusBadge status={health.ollama.status} />
                  <Tooltip content="Captures conversations with local Ollama models. Works with Open WebUI, Continue.dev, terminal, and API clients." />
                </div>
                <p className="text-sm text-cream-muted mt-1">
                  {ollamaEnabled
                    ? "Capturing Ollama conversations via Open WebUI, Continue.dev, terminal, and API clients"
                    : "Memory capture for local Ollama models (Open WebUI, Continue.dev, terminal, API)"}
                </p>
                {health.ollama.status !== "running" && ollamaEnabled && <ServiceOfflineWarning />}
              </div>
            </div>
            <div className="shrink-0 ml-4">
              {toggling === "ollama" ? (
                <Loader2 size={20} className="animate-spin text-cream-dim" />
              ) : (
                <Toggle enabled={ollamaEnabled} onChange={(v) => handleToggle("ollama", v)} />
              )}
            </div>
          </div>
        </div>

        {/* Arweave Storage */}
        <div className="border border-border bg-bg p-6">
          <div className="flex items-start justify-between">
            <div className="flex items-start gap-4">
              <ArweaveIcon />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-xl font-semibold text-cream">Arweave Storage</h3>
                  <Tooltip content="Arweave provides permanent, decentralized storage. Configure a wallet in Settings to enable." />
                </div>
                <p className="text-sm text-cream-muted mt-1">
                  {arweaveEnabled
                    ? "Memories synced to permanent decentralized storage"
                    : "Memories stored locally only — configure a wallet in Settings to enable"}
                </p>
              </div>
            </div>
            <span className="text-sm font-mono text-cream-dim shrink-0 ml-4">
              {arweaveEnabled ? "Active" : "Off"}
            </span>
          </div>
        </div>

        {/* Embedding Provider */}
        <div className="border border-border bg-bg p-6">
          <div className="flex items-start justify-between">
            <div className="flex items-start gap-4">
              <EmbeddingIcon />
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-xl font-semibold text-cream">Embedding Provider</h3>
                  <Tooltip content="The embedding provider converts text into vector representations for semantic search. Change the provider in Settings." />
                </div>
                <p className="text-sm text-cream-muted mt-1">
                  {status
                    ? `${status.embedding_model} (${status.embedding_dimensions}d)`
                    : "Loading..."}
                </p>
              </div>
            </div>
            <span className="text-sm font-mono text-accent shrink-0 ml-4">
              {status ? "Active" : "—"}
            </span>
          </div>
          {status && (
            <>
              <div className="border-t border-border mt-6 mb-6" />
              <div className="grid grid-cols-3 gap-6">
                <div>
                  <p className="text-sm text-cream-muted">Model</p>
                  <p className="text-xl font-semibold text-cream mt-1">{status.embedding_model}</p>
                </div>
                <div>
                  <p className="text-sm text-cream-muted">Dimensions</p>
                  <p className="text-xl font-semibold text-cream mt-1">{status.embedding_dimensions}</p>
                </div>
                <div>
                  <p className="text-sm text-cream-muted">Indexed</p>
                  <p className="text-xl font-semibold text-cream mt-1">{status.indexed_chunks.toLocaleString()} chunks</p>
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function OllamaIcon() {
  return (
    <div className="w-8 h-8 flex items-center justify-center text-cream-dim shrink-0 mt-1">
      <svg width="24" height="32" viewBox="0 0 24 32" fill="currentColor">
        <ellipse cx="12" cy="20" rx="10" ry="12" fill="none" stroke="currentColor" strokeWidth="2" />
        <circle cx="8" cy="17" r="2" />
        <circle cx="16" cy="17" r="2" />
        <ellipse cx="12" cy="22" rx="3" ry="2" fill="none" stroke="currentColor" strokeWidth="1.5" />
      </svg>
    </div>
  );
}

function ArweaveIcon() {
  return (
    <div className="w-8 h-8 flex items-center justify-center rounded-full border border-cream-dim text-cream-dim text-sm font-semibold shrink-0 mt-1">
      a
    </div>
  );
}

function EmbeddingIcon() {
  return (
    <div className="w-8 h-8 flex items-center justify-center rounded-full border border-cream-dim text-cream-dim text-sm font-semibold shrink-0 mt-1">
      e
    </div>
  );
}
