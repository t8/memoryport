const STEPS = ["Embedding", "Integrations", "Cloud", "Done"];

export default function SetupProgress({ current }: { current: number }) {
  return (
    <div className="flex items-center gap-2 mb-8">
      {STEPS.map((label, i) => (
        <div key={label} className="flex items-center gap-2">
          <div
            className={`w-7 h-7 flex items-center justify-center text-xs font-mono font-medium ${
              i < current
                ? "bg-accent text-bg"
                : i === current
                ? "bg-cream text-bg"
                : "bg-surface text-cream-dim"
            }`}
          >
            {i < current ? "+" : i + 1}
          </div>
          <span
            className={`text-sm hidden sm:inline ${
              i === current ? "text-cream" : "text-cream-dim"
            }`}
          >
            {label}
          </span>
          {i < STEPS.length - 1 && (
            <div className={`w-8 h-px ${i < current ? "bg-accent" : "bg-border"}`} />
          )}
        </div>
      ))}
    </div>
  );
}
