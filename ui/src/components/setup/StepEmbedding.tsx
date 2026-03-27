import { useState } from "react";
import { Cpu, Cloud, Loader2 } from "lucide-react";
import { checkOllamaInstalled, installOllama, pullOllamaModel, isTauri } from "../../lib/api";

interface Props {
  onNext: (provider: string, model: string, dimensions: number, apiKey: string | null) => void;
}

export default function StepEmbedding({ onNext }: Props) {
  const [provider, setProvider] = useState<"openai" | "ollama" | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [ollamaStatus, setOllamaStatus] = useState<"checking" | "installing" | "pulling" | "ready" | "error" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [errorDetails, setErrorDetails] = useState<string | null>(null);
  const [showErrorDetails, setShowErrorDetails] = useState(false);

  async function handleOllama() {
    setProvider("ollama");
    setOllamaStatus("checking");
    setError(null);
    setErrorDetails(null);
    setShowErrorDetails(false);

    try {
      const installed = await checkOllamaInstalled();
      if (!installed) {
        setOllamaStatus("installing");
        const result = await installOllama();
        if (result.startsWith("open:")) {
          window.open(result.slice(5), "_blank");
          setError("Install Ollama from ollama.com, then click 'Check again'");
          setOllamaStatus("error");
          return;
        }
      }

      setOllamaStatus("pulling");
      await pullOllamaModel("nomic-embed-text");
      setOllamaStatus("ready");
    } catch (e) {
      const rawMsg = e instanceof Error ? e.message : String(e);
      setError("Could not set up Ollama. Make sure Ollama is installed and running.");
      setErrorDetails(rawMsg);
      setOllamaStatus("error");
    }
  }

  function handleOpenAI() {
    setProvider("openai");
    setOllamaStatus(null);
    setError(null);
    setErrorDetails(null);
    setShowErrorDetails(false);
  }

  function handleContinue() {
    if (provider === "openai") {
      // Allow proceeding with blank API key -- the env var might be set
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
          disabled={ollamaStatus === "checking" || ollamaStatus === "installing" || ollamaStatus === "pulling"}
          className={`p-5 border text-left transition-colors ${
            provider === "ollama"
              ? "border-accent bg-accent/10"
              : "border-border hover:border-border-hover"
          } disabled:opacity-60 disabled:cursor-not-allowed`}
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
          <p className="text-xs text-cream-dim mt-1">
            Leave blank if OPENAI_API_KEY is already set in your environment
          </p>
        </div>
      )}

      {provider === "ollama" && ollamaStatus && ollamaStatus !== "ready" && ollamaStatus !== "error" && (
        <div className="flex items-center gap-2 text-sm text-cream-muted">
          <Loader2 size={16} className="animate-spin text-cream" />
          {ollamaStatus === "checking" && "Checking if Ollama is installed..."}
          {ollamaStatus === "installing" && "Installing Ollama..."}
          {ollamaStatus === "pulling" && "Pulling nomic-embed-text model (this may take a minute)..."}
        </div>
      )}

      {ollamaStatus === "ready" && (
        <p className="text-sm text-accent font-mono">Ollama ready with nomic-embed-text</p>
      )}

      {error && (
        <div className="border border-error/50 bg-error/10 p-4">
          <p className="text-sm text-error font-medium">{error}</p>
          {provider === "ollama" && (
            <div className="mt-3 space-y-2">
              <p className="text-xs text-cream-muted">
                1. Install Ollama from{" "}
                <a href="https://ollama.com" target="_blank" rel="noopener noreferrer" className="underline text-cream hover:text-cream/80">
                  ollama.com
                </a>
              </p>
              <p className="text-xs text-cream-muted">2. Make sure Ollama is running</p>
              <p className="text-xs text-cream-muted">3. Click the button below to try again</p>
              <button
                onClick={handleOllama}
                className="mt-1 px-3 py-1.5 bg-surface border border-border hover:bg-surface-hover text-cream text-sm transition-colors"
              >
                Check again
              </button>
            </div>
          )}
          {errorDetails && (
            <div className="mt-3">
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
