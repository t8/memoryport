import { useEffect, useState } from "react";
import { getSettings, updateSettings, restartServer, type SettingsData } from "../lib/api";
import Toggle from "../components/Toggle";
import Tooltip from "../components/Tooltip";
import { Save, Check, RotateCw, ChevronDown } from "lucide-react";

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
    <div>
      {/* Header */}
      <div className="px-8 pt-6 flex items-start justify-between">
        <div>
          <h2 className="font-medium uppercase text-cream text-[32px] leading-[1.4]">
            Settings
          </h2>
          <p className="text-cream-muted text-base mt-2">
            Configure your Memoryport instance
          </p>
        </div>
        <div className="flex items-center gap-4 mt-2">
          <button
            onClick={handleRestart}
            disabled={restarting}
            className="flex items-center gap-2 h-12 px-6 border border-border bg-bg hover:bg-surface disabled:opacity-50 text-sm font-medium transition-colors text-cream"
          >
            <RotateCw size={18} className={restarting ? "animate-spin" : ""} />
            {restarting ? "Restarting..." : "Restart server"}
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex items-center gap-2 h-12 px-6 border border-border bg-bg hover:bg-surface disabled:opacity-50 text-sm font-medium transition-colors text-cream"
          >
            {saved ? <Check size={18} /> : <Save size={18} />}
            {saved ? "Saved" : "Save changes"}
          </button>
        </div>
      </div>

      <div className="px-8 mt-6 space-y-6 pb-8">
        {/* Embeddings */}
        <section className="border border-border bg-bg p-6 space-y-6">
          <div className="flex items-center gap-2">
            <h3 className="text-xl font-semibold text-cream">Embedding Provider</h3>
            <Tooltip content="Embeddings convert text into numbers so Memoryport can find semantically similar content. This is the engine behind search and context retrieval." />
          </div>
          <div className="grid grid-cols-2 gap-6">
            <div>
              <label className="block text-sm text-cream-muted mb-2">Provider</label>
              <div className="relative">
                <select
                  value={settings.embeddings.provider}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      embeddings: { ...settings.embeddings, provider: e.target.value },
                    })
                  }
                  className="w-full h-12 px-3 bg-surface border border-border text-sm text-cream focus:outline-none focus:border-border-hover appearance-none"
                >
                  <option value="openai">OpenAI</option>
                  <option value="ollama">Ollama</option>
                </select>
                <ChevronDown size={20} className="absolute right-3 top-1/2 -translate-y-1/2 text-cream-dim pointer-events-none" />
              </div>
            </div>
            <div>
              <label className="block text-sm text-cream-muted mb-2">Model</label>
              <input
                type="text"
                value={settings.embeddings.model}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    embeddings: { ...settings.embeddings, model: e.target.value },
                  })
                }
                className="w-full h-12 px-3 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
              />
            </div>
          </div>
          <div>
            <label className="block text-sm text-cream-muted mb-2">API Key</label>
            <input
              type="password"
              value={settings.embeddings.api_key || ""}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  embeddings: { ...settings.embeddings, api_key: e.target.value || null },
                })
              }
              placeholder={
                settings.embeddings.provider === "ollama"
                  ? "Not required for Ollama"
                  : "sk-... (or set OPENAI_API_KEY)"
              }
              className="w-full h-12 px-3 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
            />
          </div>
        </section>

        {/* Retrieval */}
        <section className="border border-border bg-bg p-6">
          <div className="flex items-center gap-2 mb-6">
            <h3 className="text-xl font-semibold text-cream">Retrieval</h3>
            <Tooltip content="Controls how Memoryport decides what context to surface. Smart gating prevents unnecessary searches on simple messages." />
          </div>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-base font-semibold text-cream">Smart Gating</p>
              <p className="text-sm text-cream-muted mt-1">
                Skip retrieval for greetings, commands, and short queries
              </p>
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
        <section className="border border-border bg-bg p-6">
          <div className="flex items-center gap-2 mb-6">
            <h3 className="text-xl font-semibold text-cream">Proxy</h3>
            <Tooltip content="The proxy sits between your editor and the AI provider, injecting relevant context and capturing conversations automatically." />
          </div>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-base font-semibold text-cream">Multi-turn Retrieval</p>
              <p className="text-sm text-cream-muted mt-1">
                Let the LLM iteratively query memory with tool calls before responding
              </p>
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
        <section className="border border-border bg-bg p-6 space-y-4">
          <div className="flex items-center gap-2">
            <h3 className="text-xl font-semibold text-cream">Arweave Storage</h3>
            <Tooltip content="Arweave provides permanent, decentralized storage. A Pro subscription at memoryport.ai includes Turbo credits for uploads." />
          </div>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-base font-semibold text-cream">Enable Arweave Backup</p>
              <p className="text-sm text-cream-muted mt-1">
                Permanently store memories on Arweave (requires Pro API key)
              </p>
            </div>
            <Toggle
              enabled={settings.arweave.enabled}
              onChange={(v) =>
                setSettings({
                  ...settings,
                  arweave: { ...settings.arweave, enabled: v },
                })
              }
            />
          </div>
          <div>
            <label className="block text-sm text-cream-muted mb-2">API Key</label>
            <input
              type="password"
              value={settings.arweave.api_key || ""}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  arweave: { ...settings.arweave, api_key: e.target.value || null },
                })
              }
              placeholder="ex. uc_xxxx..."
              className="w-full h-12 px-3 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
            />
            <p className="text-sm text-cream-dim mt-2">
              Get a key at{" "}
              <a href="https://memoryport.ai" target="_blank" rel="noopener" className="text-cream-muted hover:text-cream underline">
                memoryport.ai
              </a>
              {" "}&ndash; or set UC_API_KEY env var
            </p>
          </div>
          {settings.arweave.address && (
            <div>
              <label className="block text-sm text-cream-muted mb-2">Arweave Address</label>
              <div className="h-12 flex items-center px-3 bg-surface border border-border text-sm text-cream-muted font-mono truncate">
                {settings.arweave.address}
              </div>
              <p className="text-sm text-cream-dim mt-2">Auto-generated signing key for Arweave uploads</p>
            </div>
          )}
        </section>

        {/* Encryption */}
        <section className="border border-border bg-bg p-6">
          <div className="flex items-center gap-2 mb-6">
            <h3 className="text-xl font-semibold text-cream">Encryption</h3>
            <Tooltip content="When enabled, all data uploaded to Arweave is encrypted with AES-256-GCM. Each batch gets a unique key." />
          </div>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-base font-semibold text-cream">Encrypt at rest</p>
              <p className="text-sm text-cream-muted mt-1">
                AES-256-GCM encryption for all Arweave uploads
              </p>
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
    </div>
  );
}
