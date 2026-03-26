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
        <h2 className="text-xl font-bold">Choose your embedding provider</h2>
        <p className="text-zinc-400 text-sm mt-1">
          Embeddings convert text into vectors so Memoryport can find similar content.
        </p>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <button
          onClick={handleOpenAI}
          className={`p-5 rounded-lg border text-left transition-colors ${
            provider === "openai"
              ? "border-emerald-500 bg-emerald-500/10"
              : "border-zinc-700 hover:border-zinc-600"
          }`}
        >
          <Cloud size={24} className="text-zinc-300 mb-3" />
          <h3 className="font-medium">OpenAI</h3>
          <p className="text-xs text-zinc-500 mt-1">Cloud embeddings. Requires API key.</p>
        </button>

        <button
          onClick={handleOllama}
          className={`p-5 rounded-lg border text-left transition-colors ${
            provider === "ollama"
              ? "border-emerald-500 bg-emerald-500/10"
              : "border-zinc-700 hover:border-zinc-600"
          }`}
        >
          <Cpu size={24} className="text-zinc-300 mb-3" />
          <h3 className="font-medium">Ollama</h3>
          <p className="text-xs text-zinc-500 mt-1">Local embeddings. Free, private.</p>
        </button>
      </div>

      {provider === "openai" && (
        <div>
          <label className="block text-sm text-zinc-400 mb-1.5">OpenAI API Key</label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-... (or set OPENAI_API_KEY env var)"
            className="w-full px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-md text-sm focus:outline-none focus:border-zinc-500"
          />
          <p className="text-xs text-zinc-600 mt-1">Optional if OPENAI_API_KEY is already set</p>
        </div>
      )}

      {provider === "ollama" && ollamaStatus && ollamaStatus !== "ready" && ollamaStatus !== "error" && (
        <div className="flex items-center gap-2 text-sm text-zinc-400">
          <div className="w-4 h-4 border-2 border-zinc-600 border-t-zinc-300 rounded-full animate-spin" />
          {ollamaStatus === "checking" && "Checking if Ollama is installed..."}
          {ollamaStatus === "installing" && "Installing Ollama..."}
          {ollamaStatus === "pulling" && "Pulling nomic-embed-text model..."}
        </div>
      )}

      {ollamaStatus === "ready" && (
        <p className="text-sm text-emerald-400">Ollama ready with nomic-embed-text</p>
      )}

      {error && (
        <div className="text-sm text-red-400">
          {error}
          {provider === "ollama" && (
            <button onClick={handleOllama} className="ml-2 underline text-zinc-400">
              Check again
            </button>
          )}
        </div>
      )}

      <button
        onClick={handleContinue}
        disabled={!canContinue}
        className="w-full py-2.5 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-30 disabled:cursor-not-allowed rounded-md text-sm font-medium transition-colors"
      >
        Continue
      </button>
    </div>
  );
}
