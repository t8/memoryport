import { useState } from "react";
import { writeInitialConfig, initEngine, startServices } from "../../lib/api";
import { setTelemetryEnabled, events } from "../../lib/telemetry";
import { Check, Loader2, AlertTriangle } from "lucide-react";

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
  const [status, setStatus] = useState<"ready" | "writing" | "starting" | "services" | "done" | "error">("ready");
  const [error, setError] = useState<string | null>(null);
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [showErrorDetails, setShowErrorDetails] = useState(false);
  const [failedAt, setFailedAt] = useState<"write" | "engine" | "services" | null>(null);

  async function handleLaunch() {
    setError(null);
    setErrorDetails(null);
    setShowErrorDetails(false);

    try {
      // Step 1: Write config (skip if we already passed this step)
      if (failedAt !== "engine" && failedAt !== "services") {
        setStatus("writing");
        setFailedAt(null);
        await writeInitialConfig({
          provider,
          model,
          dimensions,
          api_key: apiKey,
          uc_api_key: ucApiKey,
        });
      }

      // Step 2: Init engine
      if (failedAt !== "services") {
        setStatus("starting");
        setFailedAt(null);
        await initEngine();
      }

      // Step 3: Start services
      setStatus("services");
      setFailedAt(null);
      await startServices();

      setStatus("done");
      events.setupCompleted(provider);
      setTimeout(onComplete, 1000);
    } catch (e) {
      const rawMsg = e instanceof Error ? e.message : String(e);
      // Determine which step failed for retry logic
      if (status === "writing") {
        setError("Failed to write configuration file.");
        setFailedAt("write");
      } else if (status === "starting") {
        setError("Failed to initialize the Memoryport engine.");
        setFailedAt("engine");
      } else {
        setError("Failed to start background services.");
        setFailedAt("services");
      }
      setErrorDetails(rawMsg);
      setStatus("error");
    }
  }

  const isRunning = status === "writing" || status === "starting" || status === "services";

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

      <label className="flex items-start gap-3 cursor-pointer">
        <input
          type="checkbox"
          defaultChecked={false}
          onChange={(e) => setTelemetryEnabled(e.target.checked)}
          className="mt-1 accent-accent"
        />
        <span className="text-sm text-cream-muted">
          Help improve Memoryport by sending anonymous usage data (no conversations, no PII)
        </span>
      </label>

      {error && (
        <div className="border border-error/50 bg-error/10 p-4">
          <div className="flex items-start gap-2">
            <AlertTriangle size={16} className="text-error mt-0.5 shrink-0" />
            <div className="min-w-0">
              <p className="text-sm text-error font-medium">{error}</p>
              <p className="text-xs text-cream-muted mt-1">
                Click &ldquo;Retry&rdquo; to try again
                {failedAt === "engine" && " (config was saved successfully)"}
                {failedAt === "services" && " (engine initialized successfully)"}
                .
              </p>
              {errorDetails && (
                <div className="mt-2">
                  <button
                    onClick={() => setShowErrorDetails(!showErrorDetails)}
                    className="text-xs text-cream-dim hover:text-cream-muted transition-colors"
                  >
                    {showErrorDetails ? "Hide" : "Show"} technical details
                  </button>
                  {showErrorDetails && (
                    <pre className="mt-1 text-xs text-cream-dim bg-bg/50 p-2 overflow-x-auto font-mono">
                      {errorDetails}
                    </pre>
                  )}
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      <button
        onClick={handleLaunch}
        disabled={isRunning || status === "done"}
        className="w-full py-3 bg-cream text-bg hover:bg-cream/90 disabled:opacity-50 text-sm font-medium transition-colors flex items-center justify-center gap-2"
      >
        {status === "writing" && <><Loader2 size={16} className="animate-spin" /> Writing config...</>}
        {status === "starting" && <><Loader2 size={16} className="animate-spin" /> Starting engine...</>}
        {status === "services" && <><Loader2 size={16} className="animate-spin" /> Starting services...</>}
        {status === "done" && <><Check size={16} /> Done — opening dashboard</>}
        {status === "error" && "Retry"}
        {status === "ready" && "Launch Memoryport"}
      </button>
    </div>
  );
}
