import { useEffect, useState } from "react";
import { getSettings, updateSettings, restartServer, type SettingsData } from "../lib/api";
import Toggle from "../components/Toggle";
import Tooltip from "../components/Tooltip";
import { Save, Check, RotateCw } from "lucide-react";

export default function Settings() {
  const [settings, setSettings] = useState<SettingsData | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getSettings().then(setSettings).catch((e) => setError(e.message));
  }, []);

  async function handleSave() {
    if (!settings) return;
    setSaving(true);
    try {
      await updateSettings(settings);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setSaving(false);
    }
  }

  async function handleRestart() {
    setRestarting(true);
    try {
      await restartServer();
    } catch {
      // Expected — server exits before responding
    }
    // Poll health until it comes back
    const poll = async () => {
      for (let i = 0; i < 30; i++) {
        await new Promise((r) => setTimeout(r, 1000));
        try {
          const res = await fetch("/health");
          if (res.ok) {
            setRestarting(false);
            // Reload settings
            getSettings().then(setSettings);
            return;
          }
        } catch {
          // Still down
        }
      }
      setRestarting(false);
      setError("Server did not come back after restart");
    };
    poll();
  }

  if (error) {
    return (
      <div className="p-8">
        <p className="text-error">Failed to load settings: {error}</p>
      </div>
    );
  }

  if (!settings) {
    return <div className="p-8 text-cream-muted">Loading settings...</div>;
  }

  return (
    <div className="p-8 max-w-3xl space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="font-display uppercase text-cream text-2xl tracking-wide">Settings</h2>
          <p className="text-cream-muted text-sm mt-1">
            Configure your Memoryport instance
          </p>
        </div>
        <div className="flex items-center gap-3">
          <button
            onClick={handleRestart}
            disabled={restarting}
            className="flex items-center gap-2 px-4 py-2 border border-border bg-bg hover:bg-surface disabled:opacity-50 text-sm font-medium transition-colors text-cream"
          >
            <RotateCw size={16} className={restarting ? "animate-spin" : ""} />
            {restarting ? "Restarting..." : "Restart Server"}
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex items-center gap-2 px-4 py-2 bg-cream text-bg hover:bg-cream/90 disabled:opacity-50 text-sm font-medium transition-colors"
          >
            {saved ? <Check size={16} /> : <Save size={16} />}
            {saved ? "Saved" : "Save Changes"}
          </button>
        </div>
      </div>

      {/* Embeddings */}
      <section className="border border-border bg-bg p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium text-cream">Embedding Provider</h3>
          <Tooltip content="Embeddings convert text into numbers so Memoryport can find semantically similar content. This is the engine behind search and context retrieval." />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="block text-xs text-cream-dim font-mono mb-1">Provider</label>
            <select
              value={settings.embeddings.provider}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  embeddings: { ...settings.embeddings, provider: e.target.value },
                })
              }
              className="w-full px-3 py-2 bg-surface border border-border text-sm text-cream focus:outline-none focus:border-border-hover"
            >
              <option value="openai">OpenAI</option>
              <option value="ollama">Ollama</option>
            </select>
          </div>
          <div>
            <label className="block text-xs text-cream-dim font-mono mb-1">Model</label>
            <input
              type="text"
              value={settings.embeddings.model}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  embeddings: { ...settings.embeddings, model: e.target.value },
                })
              }
              className="w-full px-3 py-2 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
            />
          </div>
        </div>
        <div>
          <label className="block text-xs text-cream-dim font-mono mb-1">
            API Key {settings.embeddings.provider === "ollama" && "(not needed for Ollama)"}
          </label>
          <input
            type="password"
            value={settings.embeddings.api_key || ""}
            onChange={(e) =>
              setSettings({
                ...settings,
                embeddings: { ...settings.embeddings, api_key: e.target.value || null },
              })
            }
            placeholder={settings.embeddings.provider === "openai" ? "sk-... (or set OPENAI_API_KEY)" : "Not required"}
            className="w-full px-3 py-2 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
          />
        </div>
      </section>

      {/* Retrieval */}
      <section className="border border-border bg-bg p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium text-cream">Retrieval</h3>
          <Tooltip content="Controls how Memoryport decides what context to surface. Smart gating prevents unnecessary searches on simple messages like greetings or commands." />
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-cream">Smart Gating</p>
            <p className="text-xs text-cream-dim">Skip retrieval for greetings, commands, and short queries</p>
          </div>
          <Toggle
            enabled={settings.retrieval.gating_enabled}
            onChange={(v) =>
              setSettings({
                ...settings,
                retrieval: { ...settings.retrieval, gating_enabled: v },
              })
            }
          />
        </div>
      </section>

      {/* Proxy */}
      <section className="border border-border bg-bg p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium text-cream">Proxy</h3>
          <Tooltip content="The proxy sits between your editor and the AI provider, injecting relevant context and capturing conversations automatically." />
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-cream">Multi-turn Retrieval</p>
            <p className="text-xs text-cream-dim">Let the LLM iteratively query memory with tool calls before responding</p>
          </div>
          <Toggle
            enabled={settings.proxy?.agentic_enabled ?? true}
            onChange={(v) =>
              setSettings({
                ...settings,
                proxy: { ...settings.proxy, agentic_enabled: v },
              })
            }
          />
        </div>
      </section>

      {/* Arweave */}
      <section className="border border-border bg-bg p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium text-cream">Arweave Storage</h3>
          <Tooltip content="Arweave provides permanent, decentralized storage. A Pro subscription at memoryport.ai includes Turbo credits for uploads. Without an API key, memories are stored locally only." />
        </div>
        <div>
          <label className="block text-xs text-cream-dim font-mono mb-1">API Key</label>
          <input
            type="password"
            value={settings.arweave.api_key || ""}
            onChange={(e) =>
              setSettings({
                ...settings,
                arweave: { ...settings.arweave, api_key: e.target.value || null },
              })
            }
            placeholder="uc_... (from memoryport.ai/dashboard)"
            className="w-full px-3 py-2 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
          />
          <p className="text-xs text-cream-dim mt-1">
            Get a key at{" "}
            <a href="https://memoryport.ai/dashboard" target="_blank" rel="noopener" className="text-cream-muted hover:text-cream underline">
              memoryport.ai
            </a>
            {" "}— or set UC_API_KEY env var
          </p>
        </div>
        {settings.arweave.address && (
          <div>
            <label className="block text-xs text-cream-dim font-mono mb-1">Arweave Address</label>
            <div className="px-3 py-2 bg-surface border border-border text-sm text-cream-muted font-mono truncate">
              {settings.arweave.address}
            </div>
            <p className="text-xs text-cream-dim mt-1">Auto-generated signing key for Arweave uploads</p>
          </div>
        )}
      </section>

      {/* Encryption */}
      <section className="border border-border bg-bg p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium text-cream">Encryption</h3>
          <Tooltip content="When enabled, all data uploaded to Arweave is encrypted with AES-256-GCM. Each batch gets a unique key. You can logically delete data by destroying its encryption key." />
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-cream">Encrypt at rest</p>
            <p className="text-xs text-cream-dim">AES-256-GCM encryption for all Arweave uploads</p>
          </div>
          <Toggle
            enabled={settings.encryption.enabled}
            onChange={(v) =>
              setSettings({
                ...settings,
                encryption: { ...settings.encryption, enabled: v },
              })
            }
          />
        </div>
      </section>
    </div>
  );
}
