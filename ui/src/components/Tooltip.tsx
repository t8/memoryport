import { useState, type ReactNode } from "react";
import { Info } from "lucide-react";

interface TooltipProps {
  content: string;
  children?: ReactNode;
  align?: "center" | "left" | "right";
}

export default function Tooltip({ content, children, align = "center" }: TooltipProps) {
  const [show, setShow] = useState(false);

  const positionClass =
    align === "right" ? "right-0" :
    align === "left" ? "left-0" :
    "left-1/2 -translate-x-1/2";

  const arrowClass =
    align === "right" ? "right-2" :
    align === "left" ? "left-2" :
    "left-1/2 -translate-x-1/2";

  return (
    <span
      className="relative inline-flex items-center"
      onMouseEnter={() => setShow(true)}
      onMouseLeave={() => setShow(false)}
    >
      {children || <Info size={14} className="text-cream-dim cursor-help" />}
      {show && (
        <span className={`absolute bottom-full ${positionClass} mb-2 px-3 py-2 bg-surface border border-border rounded text-xs text-cream-muted whitespace-normal w-64 z-50 shadow-lg`}>
          {content}
          <span className={`absolute top-full ${arrowClass} -mt-px border-4 border-transparent border-t-border`} />
        </span>
      )}
    </span>
  );
}
