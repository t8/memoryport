import { useState } from "react";
import { Cpu, Cloud } from "lucide-react";
import { checkOllamaInstalled, installOllama, pullOllamaModel, isTauri } from "../../lib/api";

interface Props {
  onNext: (provider: string, model: string, dimensions: number, apiKey: string | null) => void;
}

export default function StepEmbedding({ onNext }: Props) {
  const [provider, setProvider] = useState<"openai" | "ollama" | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [ollamaStatus, setOllamaStatus] = useState<"checking" | "installing" | "pulling" | "ready" | "error" | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleOllama() {
    setProvider("ollama");
    setOllamaStatus("checking");
    setError(null);

    try {
      const installed = await checkOllamaInstalled();
      if (!installed) {
        setOllamaStatus("installing");
        const result = await installOllama();
        if (result.startsWith("open:")) {
          window.open(result.slice(5), "_blank");
          setError("Please install Ollama from the website, then click 'Check again'");
          setOllamaStatus(null);
          return;
        }
      }

      setOllamaStatus("pulling");
      await pullOllamaModel("nomic-embed-text");
      setOllamaStatus("ready");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to set up Ollama");
      setOllamaStatus("error");
    }
  }

  function handleOpenAI() {
    setProvider("openai");
    setOllamaStatus(null);
    setError(null);
  }

  function handleContinue() {
    if (provider === "openai") {
      onNext("openai", "text-embedding-3-small", 1536, apiKey || null);
    } else if (provider === "ollama" && ollamaStatus === "ready") {
      onNext("ollama", "nomic-embed-text", 768, null);
    }
  }

  const canContinue =
    (provider === "openai") ||
    (provider === "ollama" && ollamaStatus === "ready");

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-cream">Choose your embedding provider</h2>
        <p className="text-cream-muted text-sm mt-1">
          Embeddings convert text into vectors so Memoryport can find similar content.
        </p>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <button
          onClick={handleOpenAI}
          className={`p-5 border text-left transition-colors ${
            provider === "openai"
              ? "border-accent bg-accent/10"
              : "border-border hover:border-border-hover"
          }`}
        >
          <Cloud size={24} className="text-cream-muted mb-3" />
          <h3 className="font-medium text-cream">OpenAI</h3>
          <p className="text-xs text-cream-dim mt-1">Cloud embeddings. Requires API key.</p>
        </button>

        <button
          onClick={handleOllama}
          className={`p-5 border text-left transition-colors ${
            provider === "ollama"
              ? "border-accent bg-accent/10"
              : "border-border hover:border-border-hover"
          }`}
        >
          <Cpu size={24} className="text-cream-muted mb-3" />
          <h3 className="font-medium text-cream">Ollama</h3>
          <p className="text-xs text-cream-dim mt-1">Local embeddings. Free, private.</p>
        </button>
      </div>

      {provider === "openai" && (
        <div>
          <label className="block text-sm text-cream-muted mb-1.5">OpenAI API Key</label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-... (or set OPENAI_API_KEY env var)"
            className="w-full px-3 py-2 bg-surface border border-border text-sm text-cream placeholder:text-cream-dim focus:outline-none focus:border-border-hover"
          />
          <p className="text-xs text-cream-dim mt-1">Optional if OPENAI_API_KEY is already set</p>
        </div>
      )}

      {provider === "ollama" && ollamaStatus && ollamaStatus !== "ready" && ollamaStatus !== "error" && (
        <div className="flex items-center gap-2 text-sm text-cream-muted">
          <div className="w-4 h-4 border-2 border-cream-dim border-t-cream rounded-full animate-spin" />
          {ollamaStatus === "checking" && "Checking if Ollama is installed..."}
          {ollamaStatus === "installing" && "Installing Ollama..."}
          {ollamaStatus === "pulling" && "Pulling nomic-embed-text model..."}
        </div>
      )}

      {ollamaStatus === "ready" && (
        <p className="text-sm text-accent font-mono">Ollama ready with nomic-embed-text</p>
      )}

      {error && (
        <div className="text-sm text-error">
          {error}
          {provider === "ollama" && (
            <button onClick={handleOllama} className="ml-2 underline text-cream-muted">
              Check again
            </button>
          )}
        </div>
      )}

      <button
        onClick={handleContinue}
        disabled={!canContinue}
        className="w-full py-2.5 bg-cream text-bg hover:bg-cream/90 disabled:opacity-30 disabled:cursor-not-allowed text-sm font-medium transition-colors"
      >
        Continue
      </button>
    </div>
  );
}
