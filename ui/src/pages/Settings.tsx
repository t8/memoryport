import { useEffect, useState } from "react";
import { getSettings, updateSettings, type SettingsData } from "../lib/api";
import Toggle from "../components/Toggle";
import Tooltip from "../components/Tooltip";
import { Save, Check } from "lucide-react";

export default function Settings() {
  const [settings, setSettings] = useState<SettingsData | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
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

  if (error) {
    return (
      <div className="p-8">
        <p className="text-red-400">Failed to load settings: {error}</p>
      </div>
    );
  }

  if (!settings) {
    return <div className="p-8 text-zinc-500">Loading settings...</div>;
  }

  return (
    <div className="p-8 max-w-3xl space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">Settings</h2>
          <p className="text-zinc-500 text-sm mt-1">
            Configure your Memoryport instance
          </p>
        </div>
        <button
          onClick={handleSave}
          disabled={saving}
          className="flex items-center gap-2 px-4 py-2 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 rounded-md text-sm font-medium transition-colors"
        >
          {saved ? <Check size={16} /> : <Save size={16} />}
          {saved ? "Saved" : "Save Changes"}
        </button>
      </div>

      {/* Embeddings */}
      <section className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium">Embedding Provider</h3>
          <Tooltip content="Embeddings convert text into numbers so Memoryport can find semantically similar content. This is the engine behind search and context retrieval." />
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="block text-xs text-zinc-500 mb-1">Provider</label>
            <select
              value={settings.embeddings.provider}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  embeddings: { ...settings.embeddings, provider: e.target.value },
                })
              }
              className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-md text-sm focus:outline-none focus:border-zinc-500"
            >
              <option value="openai">OpenAI</option>
              <option value="ollama">Ollama</option>
            </select>
          </div>
          <div>
            <label className="block text-xs text-zinc-500 mb-1">Model</label>
            <input
              type="text"
              value={settings.embeddings.model}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  embeddings: { ...settings.embeddings, model: e.target.value },
                })
              }
              className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-md text-sm focus:outline-none focus:border-zinc-500"
            />
          </div>
        </div>
        <div>
          <label className="block text-xs text-zinc-500 mb-1">
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
            className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-md text-sm focus:outline-none focus:border-zinc-500"
          />
        </div>
      </section>

      {/* Retrieval */}
      <section className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium">Retrieval</h3>
          <Tooltip content="Controls how Memoryport decides what context to surface. Smart gating prevents unnecessary searches on simple messages like greetings or commands." />
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm">Smart Gating</p>
            <p className="text-xs text-zinc-500">Skip retrieval for greetings, commands, and short queries</p>
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

      {/* Arweave */}
      <section className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium">Arweave Storage</h3>
          <Tooltip content="Arweave provides permanent, decentralized storage. Your data is stored once and persists forever. Without a wallet, memories are stored locally only." />
        </div>
        <div>
          <label className="block text-xs text-zinc-500 mb-1">Wallet Path</label>
          <input
            type="text"
            value={settings.arweave.wallet_path || ""}
            onChange={(e) =>
              setSettings({
                ...settings,
                arweave: { ...settings.arweave, wallet_path: e.target.value || null },
              })
            }
            placeholder="~/.memoryport/wallet.json (optional)"
            className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-md text-sm focus:outline-none focus:border-zinc-500"
          />
          <p className="text-xs text-zinc-600 mt-1">Without a wallet, memories are stored locally only</p>
        </div>
      </section>

      {/* Encryption */}
      <section className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-5 space-y-4">
        <div className="flex items-center gap-2">
          <h3 className="font-medium">Encryption</h3>
          <Tooltip content="When enabled, all data uploaded to Arweave is encrypted with AES-256-GCM. Each batch gets a unique key. You can logically delete data by destroying its encryption key." />
        </div>
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm">Encrypt at rest</p>
            <p className="text-xs text-zinc-500">AES-256-GCM encryption for all Arweave uploads</p>
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
