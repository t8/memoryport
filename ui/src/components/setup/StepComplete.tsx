import { useState } from "react";
import { writeInitialConfig, initEngine, startServices } from "../../lib/api";
import { Check, Loader2 } from "lucide-react";

interface Props {
  provider: string;
  model: string;
  dimensions: number;
  apiKey: string | null;
  ucApiKey: string | null;
  onComplete: () => void;
}

export default function StepComplete({
  provider,
  model,
  dimensions,
  apiKey,
  ucApiKey,
  onComplete,
}: Props) {
  const [status, setStatus] = useState<"ready" | "writing" | "starting" | "done" | "error">("ready");
  const [error, setError] = useState<string | null>(null);

  async function handleLaunch() {
    setStatus("writing");
    setError(null);

    try {
      await writeInitialConfig({
        provider,
        model,
        dimensions,
        api_key: apiKey,
        uc_api_key: ucApiKey,
      });

      setStatus("starting");
      await initEngine();
      await startServices();

      setStatus("done");
      setTimeout(onComplete, 1000);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Setup failed");
      setStatus("error");
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-cream">Ready to go</h2>
        <p className="text-cream-muted text-sm mt-1">
          Here&apos;s what we&apos;ll set up:
        </p>
      </div>

      <div className="space-y-2 text-sm">
        <div className="flex items-center gap-2">
          <Check size={14} className="text-accent" />
          <span className="text-cream">
            Embedding: <strong>{provider}</strong> / {model} ({dimensions}d)
          </span>
        </div>
        {ucApiKey && (
          <div className="flex items-center gap-2">
            <Check size={14} className="text-accent" />
            <span className="text-cream">Arweave Pro storage enabled</span>
          </div>
        )}
        <div className="flex items-center gap-2">
          <Check size={14} className="text-accent" />
          <span className="text-cream font-mono">Config at ~/.memoryport/uc.toml</span>
        </div>
      </div>

      {error && (
        <p className="text-sm text-error">{error}</p>
      )}

      <button
        onClick={handleLaunch}
        disabled={status === "writing" || status === "starting" || status === "done"}
        className="w-full py-3 bg-cream text-bg hover:bg-cream/90 disabled:opacity-50 text-sm font-medium transition-colors flex items-center justify-center gap-2"
      >
        {status === "writing" && <><Loader2 size={16} className="animate-spin" /> Writing config...</>}
        {status === "starting" && <><Loader2 size={16} className="animate-spin" /> Starting engine...</>}
        {status === "done" && <><Check size={16} /> Done — opening dashboard</>}
        {status === "error" && "Retry"}
        {status === "ready" && "Launch Memoryport"}
      </button>
    </div>
  );
}
