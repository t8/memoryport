const STEPS = ["Embedding", "Integrations", "Cloud", "Done"];

export default function SetupProgress({ current }: { current: number }) {
  return (
    <div className="flex items-center gap-2 mb-8">
      {STEPS.map((label, i) => (
        <div key={label} className="flex items-center gap-2">
          <div
            className={`w-7 h-7 rounded-full flex items-center justify-center text-xs font-medium ${
              i < current
                ? "bg-emerald-600 text-white"
                : i === current
                ? "bg-zinc-100 text-zinc-900"
                : "bg-zinc-800 text-zinc-500"
            }`}
          >
            {i < current ? "✓" : i + 1}
          </div>
          <span
            className={`text-sm hidden sm:inline ${
              i === current ? "text-zinc-100" : "text-zinc-500"
            }`}
          >
            {label}
          </span>
          {i < STEPS.length - 1 && (
            <div className={`w-8 h-px ${i < current ? "bg-emerald-600" : "bg-zinc-700"}`} />
          )}
        </div>
      ))}
    </div>
  );
}
