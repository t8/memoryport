import { useEffect, useState, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { getSettings, updateSettings, rebuildFromArweave, resetAllData, validateApiKey, exportWallet, importWallet, restartAllServices, type SettingsData } from "../lib/api";
import Toggle from "../components/Toggle";
import Tooltip from "../components/Tooltip";
import { Check, RotateCw, ChevronDown, HardDriveDownload, Loader2, Trash2 } from "lucide-react";

export default function Settings() {
  const navigate = useNavigate();
  const [settings, setSettings] = useState<SettingsData | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [rebuilding, setRebuilding] = useState(false);
  const [rebuildResult, setRebuildResult] = useState<{ chunks_restored: number } | null>(null);
  const [rebuildError, setRebuildError] = useState<string | null>(null);
  const [resetting, setResetting] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const [validatingKey, setValidatingKey] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [generatingWallet, setGeneratingWallet] = useState(false);
  const [confirmDisableEncryption, setConfirmDisableEncryption] = useState(false);
  const saveSkipCount = useRef(0);
  const [originalEmbeddings, setOriginalEmbeddings] = useState<{ provider: string; model: string } | null>(null);

  function formatTokens(bytes: number): string {
    const tokens = Math.round(bytes / 4); // ~4 bytes per token
    if (tokens < 1000) return `${tokens}`;
    if (tokens < 1_000_000) return `${(tokens / 1000).toFixed(0)}K`;
    if (tokens < 1_000_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`;
    return `${(tokens / 1_000_000_000).toFixed(1)}B`;
  }

  async function handleRebuild() {
    setRebuilding(true);
    setRebuildResult(null);
    setRebuildError(null);
    try {
      const result = await rebuildFromArweave();
      setRebuildResult(result);
    } catch (e: any) {
      setRebuildError(e.message);
    } finally {
      setRebuilding(false);
    }
  }

  useEffect(() => {
    getSettings().then((s) => {
      setSettings(s);
      setOriginalEmbeddings({ provider: s.embeddings.provider, model: s.embeddings.model });
    }).catch((e) => setError(e.message));
  }, []);

  // Auto-validate Arweave API key when it looks like a valid key
  useEffect(() => {
    const key = settings?.arweave.api_key;
    if (!key || !key.startsWith("uc_") || key.length < 10) return;
    // Skip if already validated (has storage_limit_bytes)
    if (settings.arweave.storage_limit_bytes != null) return;

    const timer = setTimeout(async () => {
      setValidatingKey(true);
      setKeyError(null);
      try {
        const result = await validateApiKey(key);
        if (!result.valid) {
          setKeyError("Invalid API key — check your key at memoryport.ai");
          return;
        }
        // Save key to config and enable Arweave
        const updated = {
          ...settings,
          arweave: {
            ...settings.arweave,
            api_key: key,
            enabled: true,
            storage_used_bytes: result.storage_used_bytes ?? 0,
            storage_limit_bytes: result.storage_limit_bytes ?? 1073741824,
          },
        };
        await updateSettings(updated);
        setSettings(updated);
        setValidatingKey(false);

        // Wallet is auto-generated on engine restart — show progress
        if (!updated.arweave.address) {
          setGeneratingWallet(true);
          // Poll for wallet address to appear
          for (let i = 0; i < 15; i++) {
            await new Promise((r) => setTimeout(r, 1000));
            try {
              const fresh = await getSettings();
              if (fresh.arweave.address) {
                setSettings({ ...fresh, arweave: { ...fresh.arweave, api_key: key } });
                setGeneratingWallet(false);
                return;
              }
            } catch { /* keep polling */ }
          }
          setGeneratingWallet(false);
        }
        return;
      } catch (e: any) {
        setKeyError(e.message || "Failed to validate key");
      } finally {
        setValidatingKey(false);
      }
    }, 800);
    return () => clearTimeout(timer);
  }, [settings?.arweave.api_key]);

  // Auto-save settings when user makes changes (debounced, skip first 2 renders)
  useEffect(() => {
    if (!settings) return;
    if (saveSkipCount.current < 2) {
      saveSkipCount.current++;
      return;
    }
    const timer = setTimeout(async () => {
      setSaving(true);
      try {
        await updateSettings(settings);
        setSaved(true);
        setTimeout(() => setSaved(false), 1500);
      } catch {
        // Ignore auto-save errors
      } finally {
        setSaving(false);
      }
    }, 1000);
    return () => clearTimeout(timer);
  }, [settings]);

  async function handleRestart() {
    setRestarting(true);
    try {
      await restartAllServices();
    } catch {
      // ignore
    }
    setTimeout(() => {
      setRestarting(false);
      getSettings().then(setSettings).catch(() => {});
    }, 3000);
  }


  if (error && !settings) {
    return (
      <div className="p-8 space-y-4">
        <p className="text-error">Failed to load settings: {error}</p>
        <button
          onClick={() => {
            setError(null);
            getSettings().then(setSettings).catch((e) => setError(e.message));
          }}
          className="flex items-center gap-2 px-4 py-2 border border-border bg-bg hover:bg-surface text-sm text-cream transition-colors"
        >
          <RotateCw size={14} />
          Retry
        </button>
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
          {saved && (
            <span className="flex items-center gap-1.5 text-sm text-accent">
              <Check size={14} /> Saved
            </span>
          )}
          <button
            onClick={handleRestart}
            disabled={restarting}
            className="flex items-center gap-2 h-12 px-6 border border-border bg-bg hover:bg-surface disabled:opacity-50 text-sm font-medium transition-colors text-cream"
          >
            <RotateCw size={18} className={restarting ? "animate-spin" : ""} />
            {restarting ? "Restarting..." : "Restart services"}
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
          {originalEmbeddings && settings && (
            settings.embeddings.provider !== originalEmbeddings.provider ||
            settings.embeddings.model !== originalEmbeddings.model
          ) && (
            <div className="border border-error/50 bg-error/10 p-4">
              <p className="text-sm text-error font-medium">
                Changing the embedding model will make your existing memories unsearchable.
              </p>
              <p className="text-sm text-cream-muted mt-1">
                All stored vectors were computed with {originalEmbeddings.provider}/{originalEmbeddings.model}. Switching models means new embeddings will be incompatible with old ones. You will need to delete your index and re-capture all conversations.
              </p>
            </div>
          )}
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
                {settings.arweave.storage_limit_bytes != null
                  ? "Permanently store memories on Arweave"
                  : "Save settings with a valid Pro API key first"}
              </p>
            </div>
            <Toggle
              enabled={settings.arweave.enabled}
              disabled={settings.arweave.storage_limit_bytes == null}
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
              onChange={(e) => {
                setKeyError(null);
                setSettings({
                  ...settings,
                  arweave: { ...settings.arweave, api_key: e.target.value || null },
                });
              }}
              placeholder="ex. uc_xxxx..."
              className="w-full h-12 px-3 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
            />
            {generatingWallet ? (
              <p className="text-sm text-accent mt-2 flex items-center gap-1.5">
                <Loader2 size={12} className="animate-spin" /> Generating Arweave wallet...
              </p>
            ) : validatingKey ? (
              <p className="text-sm text-accent mt-2 flex items-center gap-1.5">
                <Loader2 size={12} className="animate-spin" /> Validating key...
              </p>
            ) : keyError ? (
              <p className="text-sm text-error mt-2">{keyError}</p>
            ) : settings.arweave.storage_limit_bytes != null && settings.arweave.api_key ? (
              <p className="text-sm text-accent mt-2 flex items-center gap-1.5">
                <Check size={12} /> Key validated
              </p>
            ) : (
              <p className="text-sm text-cream-dim mt-2">
                Get a key at{" "}
                <a href="https://memoryport.ai" target="_blank" rel="noopener" className="text-cream-muted hover:text-cream underline">
                  memoryport.ai
                </a>
              </p>
            )}
          </div>
          {settings.arweave.api_key && settings.arweave.storage_used_bytes != null && settings.arweave.storage_limit_bytes != null && (
            <div>
              <label className="block text-sm text-cream-muted mb-2">Storage used this month</label>
              <div className="h-8 w-full bg-surface border border-border overflow-hidden relative">
                <div
                  className="h-full bg-accent/30 transition-all duration-300"
                  style={{
                    width: `${Math.min(100, (settings.arweave.storage_used_bytes / settings.arweave.storage_limit_bytes) * 100)}%`,
                  }}
                />
                <span className="absolute inset-0 flex items-center justify-center text-xs font-mono text-cream">
                  {formatTokens(settings.arweave.storage_used_bytes)} / {formatTokens(settings.arweave.storage_limit_bytes)} tokens
                </span>
              </div>
              <p className="text-sm text-cream-dim mt-2">
                <a
                  href="https://memoryport.ai/dashboard"
                  target="_blank"
                  rel="noopener"
                  className="text-cream-muted hover:text-cream underline"
                >
                  Manage billing
                </a>
              </p>
            </div>
          )}
          {settings.arweave.address && (
            <div>
              <label className="block text-sm text-cream-muted mb-2">Arweave Wallet</label>
              <div className="h-12 flex items-center px-3 bg-surface border border-border text-sm text-cream-muted font-mono truncate">
                {settings.arweave.address}
              </div>
              <div className="flex items-center justify-between mt-3">
                <div className="flex items-center gap-2">
                  <label className="inline-flex items-center text-sm text-cream-muted hover:text-cream cursor-pointer">
                    <span className="underline">Import keyfile</span>
                    <input
                      type="file"
                      accept=".json"
                      className="hidden"
                      onChange={async (e) => {
                        const file = e.target.files?.[0];
                        if (!file) return;
                        try {
                          const text = await file.text();
                          JSON.parse(text);
                          await importWallet(text);
                          const fresh = await getSettings();
                          setSettings({ ...fresh, arweave: { ...fresh.arweave, api_key: settings.arweave.api_key } });
                        } catch (err: any) {
                          setError(`Invalid wallet file: ${err.message}`);
                        }
                      }}
                    />
                  </label>
                  <Tooltip content="Import a wallet from another device to access your existing memories. Warning: this replaces your current wallet — any memories stored with the current key will be inaccessible unless you export it first." />
                </div>
                <div className="flex items-center gap-2">
                  <ExportKeyfileButton onError={(msg) => setError(msg)} />
                  <Tooltip align="right" content="Save your wallet keyfile to a safe location. You'll need it to recover your memories on another device or if you reset this app." />
                </div>
              </div>
            </div>
          )}
          {settings.arweave.api_key && settings.arweave.storage_limit_bytes != null && (
            <div className="pt-2">
              <div className="flex items-start justify-between">
                <div>
                  <p className="text-base font-semibold text-cream">Rebuild from Arweave</p>
                  <p className="text-sm text-cream-muted mt-1">
                    Restore your memory from permanent storage on a new device
                  </p>
                </div>
                <button
                  onClick={handleRebuild}
                  disabled={rebuilding}
                  className="flex items-center gap-2 h-10 px-5 border border-border bg-bg hover:bg-surface disabled:opacity-50 text-sm font-medium transition-colors text-cream shrink-0 ml-4"
                >
                  {rebuilding ? (
                    <Loader2 size={16} className="animate-spin" />
                  ) : (
                    <HardDriveDownload size={16} />
                  )}
                  {rebuilding ? "Rebuilding..." : "Rebuild"}
                </button>
              </div>
              {rebuilding && (
                <p className="text-sm text-cream-muted mt-3">
                  Rebuilding... this may take several minutes
                </p>
              )}
              {rebuildResult && (
                <p className="text-sm text-accent mt-3">
                  Rebuild complete &mdash; {rebuildResult.chunks_restored} chunks restored
                </p>
              )}
              {rebuildError && (
                <div className="mt-3 flex items-center gap-3">
                  <p className="text-sm text-error">{rebuildError}</p>
                  <button
                    onClick={handleRebuild}
                    className="text-sm text-cream-muted hover:text-cream underline"
                  >
                    Retry
                  </button>
                </div>
              )}
            </div>
          )}
        </section>

        {/* Encryption */}
        <section className="border border-border bg-bg p-6">
          <div className="flex items-center gap-2 mb-6">
            <h3 className="text-xl font-semibold text-cream">Encryption</h3>
            <Tooltip content="When enabled, all data uploaded to Arweave is encrypted with AES-256-GCM. Each batch gets a unique key wrapped with your master passphrase. Local data is not encrypted." />
          </div>
          <div className="flex items-center justify-between">
            <div>
              <p className="text-base font-semibold text-cream">Encrypt Arweave uploads</p>
              <p className="text-sm text-cream-muted mt-1">
                {settings.arweave.storage_limit_bytes != null
                  ? "AES-256-GCM encryption for permanent storage (set UC_MASTER_PASSPHRASE env var)"
                  : "Add a valid Pro API key above to enable encryption"}
              </p>
            </div>
            <Toggle
              enabled={settings.encryption.enabled}
              disabled={settings.arweave.storage_limit_bytes == null}
              onChange={(v) => {
                if (!v && settings.encryption.enabled) {
                  setConfirmDisableEncryption(true);
                  return;
                }
                setSettings({
                  ...settings,
                  encryption: { ...settings.encryption, enabled: v },
                });
              }}
            />
          </div>
          {confirmDisableEncryption && (
            <div className="mt-4 border border-error/50 bg-error/10 p-4 space-y-3">
              <p className="text-sm text-error font-medium">
                Disabling encryption means previously encrypted data on Arweave will become permanently inaccessible during rebuild. Only disable if you have no encrypted data.
              </p>
              <div className="flex items-center gap-3">
                <button
                  onClick={() => {
                    setSettings({ ...settings, encryption: { ...settings.encryption, enabled: false } });
                    setConfirmDisableEncryption(false);
                  }}
                  className="px-4 py-1.5 bg-error text-white hover:bg-error/90 text-sm transition-colors"
                >
                  Disable anyway
                </button>
                <button
                  onClick={() => setConfirmDisableEncryption(false)}
                  className="px-4 py-1.5 border border-border text-cream-muted hover:bg-surface text-sm transition-colors"
                >
                  Keep enabled
                </button>
              </div>
            </div>
          )}
        </section>

        {/* Report Issue */}
        <section className="border border-border bg-bg p-6">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-base font-semibold text-cream">Report an issue</p>
              <p className="text-sm text-cream-muted mt-1">
                Found a bug or have a feature request? Let us know on GitHub.
              </p>
            </div>
            <a
              href="https://github.com/t8/memoryport/issues"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 h-10 px-5 border border-border bg-bg hover:bg-surface text-sm font-medium transition-colors text-cream shrink-0 ml-4"
            >
              Open issue
            </a>
          </div>
        </section>

        {/* Danger Zone */}
        <section className="border border-error/30 p-6">
          <h3 className="text-lg font-semibold text-error mb-2">Danger Zone</h3>
          <p className="text-sm text-cream-muted mb-4">
            Permanently delete all memories, configuration, index data, and your Arweave wallet. This will unregister MCP and proxy integrations and cannot be undone.
          </p>
          <button
            onClick={() => setConfirmReset(true)}
            className="flex items-center gap-2 px-4 py-2 border border-error/50 text-error hover:bg-error/10 text-sm transition-colors"
          >
            <Trash2 size={14} />
            Delete all data
          </button>
        </section>

        {/* Delete confirmation modal */}
        {confirmReset && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70">
            <div className="bg-bg border border-error/50 p-6 max-w-md w-full mx-4 space-y-4">
              <h3 className="text-lg font-semibold text-error">Delete all data?</h3>
              <div className="text-sm text-cream-muted space-y-2">
                <p>This will permanently delete:</p>
                <ul className="list-disc ml-5 space-y-1">
                  <li>All stored memories and index data</li>
                  <li>Your configuration file</li>
                  <li>Your Arweave wallet (signing key)</li>
                  <li>MCP and proxy registrations</li>
                </ul>
                <p className="text-error/80 font-medium mt-3">
                  If you are a Pro user, export your wallet keyfile first so you can recover your memories on another device.
                </p>
              </div>

              {settings?.arweave.address && settings?.arweave.storage_limit_bytes != null && (
                <ExportKeyfileButton
                  label="Export wallet keyfile first"
                  className="flex items-center gap-2 w-full px-4 py-2.5 border border-accent/50 text-accent hover:bg-accent/10 text-sm transition-colors justify-center"
                  onError={(msg) => setError(msg)}
                />
              )}

              <div className="flex items-center gap-3 pt-2">
                <button
                  onClick={async () => {
                    setResetting(true);
                    try {
                      await resetAllData();
                      navigate("/setup");
                    } catch (e: any) {
                      setError(`Reset failed: ${e.message}`);
                      setResetting(false);
                      setConfirmReset(false);
                    }
                  }}
                  disabled={resetting}
                  className="flex items-center gap-2 px-4 py-2 bg-error text-white hover:bg-error/90 disabled:opacity-50 text-sm transition-colors"
                >
                  {resetting ? <Loader2 size={14} className="animate-spin" /> : <Trash2 size={14} />}
                  {resetting ? "Deleting..." : "Delete everything"}
                </button>
                <button
                  onClick={() => setConfirmReset(false)}
                  disabled={resetting}
                  className="px-4 py-2 border border-border text-cream-muted hover:bg-surface text-sm transition-colors"
                >
                  Cancel
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ExportKeyfileButton({ label, className, onError }: { label?: string; className?: string; onError: (msg: string) => void }) {
  const [exported, setExported] = useState(false);
  const [savedMsg, setSavedMsg] = useState("");

  async function handleExport() {
    try {
      const jwk = await exportWallet();
      const blob = new Blob([jwk], { type: "application/json" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = "memoryport-wallet.json";
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      setExported(true);
      setSavedMsg("~/Downloads/memoryport-wallet.json");
      setTimeout(() => { setExported(false); setSavedMsg(""); }, 5000);
    } catch (e: any) {
      onError(`Export failed: ${e.message}`);
    }
  }

  return (
    <button type="button" onClick={handleExport} className={className || "text-sm text-cream-muted hover:text-cream underline"}>
      {exported ? (
        <span className="flex items-center gap-1.5"><Check size={14} /> Saved to {savedMsg}</span>
      ) : (
        <>{label ? <><HardDriveDownload size={14} /> {label}</> : "Export keyfile"}</>
      )}
    </button>
  );
}
