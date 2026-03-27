import { useState } from "react";

interface Props {
  onNext: (ucApiKey: string | null) => void;
}

export default function StepApiKey({ onNext }: Props) {
  const [key, setKey] = useState("");

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-cream">Cloud backup (optional)</h2>
        <p className="text-cream-muted text-sm mt-1">
          Back up your memory permanently to Arweave. Requires a Pro subscription.
        </p>
      </div>

      <div>
        <label className="block text-sm text-cream-muted mb-1.5">Memoryport Pro API Key</label>
        <input
          type="text"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="uc_... (from memoryport.ai/dashboard)"
          className="w-full px-3 py-2 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
        />
        <p className="text-xs text-cream-dim mt-1">
          Get a key at{" "}
          <a
            href="https://memoryport.ai/dashboard"
            target="_blank"
            rel="noopener"
            className="text-cream-muted hover:text-cream underline"
          >
            memoryport.ai
          </a>
        </p>
      </div>

      <div className="flex gap-3">
        <button
          onClick={() => onNext(null)}
          className="flex-1 py-2.5 border border-border bg-bg hover:bg-surface text-sm font-medium transition-colors text-cream"
        >
          Skip — local only
        </button>
        <button
          onClick={() => onNext(key.startsWith("uc_") ? key : null)}
          disabled={!key.startsWith("uc_")}
          className="flex-1 py-2.5 bg-cream text-bg hover:bg-cream/90 disabled:opacity-30 disabled:cursor-not-allowed text-sm font-medium transition-colors"
        >
          Save key
        </button>
      </div>
    </div>
  );
}
