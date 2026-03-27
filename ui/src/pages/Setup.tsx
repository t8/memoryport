import { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { checkConfigExists, isTauri } from "../lib/api";
import SetupProgress from "../components/setup/SetupProgress";
import StepEmbedding from "../components/setup/StepEmbedding";
import StepIntegrations from "../components/setup/StepIntegrations";
import StepApiKey from "../components/setup/StepApiKey";
import StepComplete from "../components/setup/StepComplete";

interface SetupState {
  provider: string;
  model: string;
  dimensions: number;
  apiKey: string | null;
  ucApiKey: string | null;
}

export default function Setup() {
  const navigate = useNavigate();
  const [step, setStep] = useState(0);
  const [stepError, setStepError] = useState<string | null>(null);
  const [configExists, setConfigExists] = useState(false);
  const [config, setConfig] = useState<SetupState>({
    provider: "",
    model: "",
    dimensions: 0,
    apiKey: null,
    ucApiKey: null,
  });

  useEffect(() => {
    if (isTauri()) {
      checkConfigExists()
        .then((exists) => setConfigExists(exists))
        .catch(() => {});
    }
  }, []);

  function safeSetStep(next: number) {
    try {
      setStepError(null);
      setStep(next);
    } catch (e) {
      setStepError(
        e instanceof Error ? e.message : "Something went wrong advancing to the next step."
      );
    }
  }

  return (
    <div className="min-h-screen bg-bg text-cream flex items-center justify-center p-8">
      <div className="w-full max-w-lg">
        <div className="mb-8">
          <h1 className="font-display uppercase text-cream text-2xl tracking-wide">Memoryport</h1>
          <p className="text-cream-muted text-sm">Setup wizard</p>
        </div>

        {configExists && (
          <div className="mb-6 border border-accent/50 bg-accent/10 p-4 flex items-center justify-between">
            <div>
              <p className="text-sm font-medium text-cream">Config already exists</p>
              <p className="text-xs text-cream-muted mt-0.5">A Memoryport configuration was found at ~/.memoryport/uc.toml</p>
            </div>
            <button
              onClick={() => navigate("/")}
              className="px-4 py-1.5 bg-cream text-bg hover:bg-cream/90 text-sm font-medium transition-colors whitespace-nowrap ml-4"
            >
              Go to Dashboard
            </button>
          </div>
        )}

        {stepError && (
          <div className="mb-6 border border-error/50 bg-error/10 p-4">
            <p className="text-sm font-medium text-error">Step transition failed</p>
            <p className="text-xs text-cream-muted mt-1">{stepError}</p>
            <button
              onClick={() => setStepError(null)}
              className="mt-2 text-xs text-cream-dim hover:text-cream transition-colors"
            >
              Dismiss
            </button>
          </div>
        )}

        <SetupProgress current={step} />

        {step === 0 && (
          <StepEmbedding
            onNext={(provider, model, dimensions, apiKey) => {
              try {
                setConfig((c) => ({ ...c, provider, model, dimensions, apiKey }));
                safeSetStep(1);
              } catch (e) {
                setStepError(e instanceof Error ? e.message : "Failed to save embedding settings.");
              }
            }}
          />
        )}

        {step === 1 && (
          <StepIntegrations
            onNext={() => {
              try {
                safeSetStep(2);
              } catch (e) {
                setStepError(e instanceof Error ? e.message : "Failed to advance past integrations.");
              }
            }}
          />
        )}

        {step === 2 && (
          <StepApiKey
            onNext={(ucApiKey) => {
              try {
                setConfig((c) => ({ ...c, ucApiKey }));
                safeSetStep(3);
              } catch (e) {
                setStepError(e instanceof Error ? e.message : "Failed to save API key settings.");
              }
            }}
          />
        )}

        {step === 3 && (
          <StepComplete
            provider={config.provider}
            model={config.model}
            dimensions={config.dimensions}
            apiKey={config.apiKey}
            ucApiKey={config.ucApiKey}
            onComplete={() => navigate("/")}
          />
        )}
      </div>
    </div>
  );
}
