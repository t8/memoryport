import { useState } from "react";
import { useNavigate } from "react-router-dom";
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
  const [config, setConfig] = useState<SetupState>({
    provider: "",
    model: "",
    dimensions: 0,
    apiKey: null,
    ucApiKey: null,
  });

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex items-center justify-center p-8">
      <div className="w-full max-w-lg">
        <div className="mb-8">
          <h1 className="text-2xl font-bold tracking-tight">Memoryport</h1>
          <p className="text-zinc-500 text-sm">Setup wizard</p>
        </div>

        <SetupProgress current={step} />

        {step === 0 && (
          <StepEmbedding
            onNext={(provider, model, dimensions, apiKey) => {
              setConfig((c) => ({ ...c, provider, model, dimensions, apiKey }));
              setStep(1);
            }}
          />
        )}

        {step === 1 && (
          <StepIntegrations onNext={() => setStep(2)} />
        )}

        {step === 2 && (
          <StepApiKey
            onNext={(ucApiKey) => {
              setConfig((c) => ({ ...c, ucApiKey }));
              setStep(3);
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
