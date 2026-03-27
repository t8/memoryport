/**
 * Lightweight opt-in telemetry.
 * Sends anonymous usage events to help improve Memoryport.
 * No PII, no conversation content. Disabled by default.
 */

const TELEMETRY_ENDPOINT = "https://memoryport.ai/api/telemetry";
const STORAGE_KEY = "memoryport_telemetry_enabled";

let enabled: boolean | null = null;

export function isTelemetryEnabled(): boolean {
  if (enabled !== null) return enabled;
  try {
    enabled = localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    enabled = false;
  }
  return enabled;
}

export function setTelemetryEnabled(value: boolean) {
  enabled = value;
  try {
    localStorage.setItem(STORAGE_KEY, value ? "true" : "false");
  } catch {
    // localStorage not available
  }
}

let sessionId: string | null = null;

function getSessionId(): string {
  if (!sessionId) {
    sessionId = Math.random().toString(36).slice(2) + Date.now().toString(36);
  }
  return sessionId;
}

export function trackEvent(event: string, properties?: Record<string, string | number | boolean>) {
  if (!isTelemetryEnabled()) return;

  const payload = {
    event,
    session_id: getSessionId(),
    timestamp: new Date().toISOString(),
    platform: typeof navigator !== "undefined" ? navigator.platform : "unknown",
    ...properties,
  };

  // Fire and forget — never block the UI
  try {
    if (navigator.sendBeacon) {
      navigator.sendBeacon(TELEMETRY_ENDPOINT, JSON.stringify(payload));
    } else {
      fetch(TELEMETRY_ENDPOINT, {
        method: "POST",
        body: JSON.stringify(payload),
        keepalive: true,
      }).catch(() => {});
    }
  } catch {
    // Silently fail
  }
}

// Standard events
export const events = {
  appOpened: () => trackEvent("app_opened"),
  pageViewed: (page: string) => trackEvent("page_viewed", { page }),
  setupCompleted: (provider: string) => trackEvent("setup_completed", { provider }),
  searchPerformed: () => trackEvent("search_performed"),
  sessionViewed: () => trackEvent("session_viewed"),
  settingsSaved: () => trackEvent("settings_saved"),
  proKeyAdded: () => trackEvent("pro_key_added"),
  rebuildStarted: () => trackEvent("rebuild_started"),
};
