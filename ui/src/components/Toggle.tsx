interface ToggleProps {
  enabled: boolean;
  onChange: (value: boolean) => void;
}

export default function Toggle({ enabled, onChange }: ToggleProps) {
  return (
    <div className="flex items-center gap-2">
      <button
        type="button"
        role="switch"
        aria-checked={enabled}
        onClick={() => onChange(!enabled)}
        className={`relative inline-flex h-5 w-10 shrink-0 cursor-pointer rounded-full transition-colors duration-200 ${
          enabled ? "bg-accent" : "bg-cream-dim"
        }`}
      >
        <span
          className={`pointer-events-none inline-block h-4 w-4 rounded-full bg-bg shadow-sm transform transition-transform duration-200 mt-0.5 ${
            enabled ? "translate-x-[22px]" : "translate-x-[2px]"
          }`}
        />
      </button>
      <span className={`text-xs font-mono ${enabled ? "text-accent" : "text-cream-dim"}`}>
        {enabled ? "Active" : "Off"}
      </span>
    </div>
  );
}
