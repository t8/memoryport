import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { isTauri } from "../lib/api";

type UpdateState = "idle" | "available" | "downloading" | "ready" | "error";

export default function UpdateBadge() {
  const [state, setState] = useState<UpdateState>("idle");
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    checkForUpdate();
  }, []);

  async function checkForUpdate() {
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (update) {
        setVersion(update.version);
        setState("available");
      }
    } catch {
      // No update or check failed — stay idle
    }
  }

  async function handleUpdate() {
    setState("downloading");
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (update) {
        await update.downloadAndInstall();
        setState("ready");
        // Relaunch after a short delay
        const { relaunch } = await import("@tauri-apps/plugin-process");
        setTimeout(() => relaunch(), 1500);
      }
    } catch {
      setState("error");
      setTimeout(() => setState("available"), 3000);
    }
  }

  if (state === "idle") return null;

  if (state === "ready") {
    return (
      <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-accent/20 text-accent">
        Restarting...
      </span>
    );
  }

  if (state === "downloading") {
    return (
      <span className="flex items-center gap-1 text-[10px] font-mono px-1.5 py-0.5 rounded bg-accent/20 text-accent">
        <Loader2 size={10} className="animate-spin" /> Updating...
      </span>
    );
  }

  if (state === "error") {
    return (
      <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-error/20 text-error">
        Update failed
      </span>
    );
  }

  return (
    <button
      onClick={handleUpdate}
      className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-accent/20 text-accent hover:bg-accent/30 transition-colors cursor-pointer"
    >
      {version} available
    </button>
  );
}
