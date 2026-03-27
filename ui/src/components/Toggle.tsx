interface ToggleProps {
  enabled: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
}

export default function Toggle({ enabled, onChange, disabled }: ToggleProps) {
  return (
    <div className={`flex items-center gap-2 ${disabled ? "opacity-40 pointer-events-none" : ""}`}>
      <span className={`text-sm font-mono ${enabled ? "text-accent" : "text-cream-dim"}`}>
        {enabled ? "Active" : "Off"}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={enabled}
        disabled={disabled}
        onClick={() => onChange(!enabled)}
        className={`relative inline-flex h-[30px] w-14 shrink-0 cursor-pointer rounded-full transition-colors duration-200 ${
          enabled ? "bg-accent" : "bg-cream-dim"
        }`}
      >
        <span
          className={`pointer-events-none inline-block h-6 w-6 rounded-full bg-white shadow-sm transform transition-transform duration-200 mt-[3px] ${
            enabled ? "translate-x-[29px]" : "translate-x-[3px]"
          }`}
        />
      </button>
    </div>
  );
}
