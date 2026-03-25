import { useEffect, useState } from "react";
import { getAnalytics, type AnalyticsData } from "../lib/api";
import Sparkline from "../components/Sparkline";
import SegmentBar from "../components/SegmentBar";
import SyncBar from "../components/SyncBar";
import ActivityHeatmap from "../components/ActivityHeatmap";
import StatusCard from "../components/StatusCard";

export default function Analytics() {
  const [data, setData] = useState<AnalyticsData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getAnalytics()
      .then(setData)
      .catch((e) => setError(e.message));
  }, []);

  if (error) {
    return (
      <div className="p-8">
        <p className="text-red-400">Failed to load analytics: {error}</p>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="p-8 text-zinc-500">Loading analytics...</div>
    );
  }

  // Compute cumulative growth from activity
  const activityCounts = data.activity.map((a) => a.count);
  const cumulativeCounts = activityCounts.reduce<number[]>((acc, v) => {
    acc.push((acc[acc.length - 1] || 0) + v);
    return acc;
  }, []);

  return (
    <div className="p-8 max-w-5xl space-y-8">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Analytics</h2>
        <p className="text-zinc-500 text-sm mt-1">
          How your memory is growing
        </p>
      </div>

      {/* Top stats */}
      <div className="grid grid-cols-3 gap-4">
        <StatusCard label="Total Chunks" value={data.total_chunks} />
        <StatusCard label="Sessions" value={data.total_sessions} />
        <StatusCard
          label="Types"
          value={Object.keys(data.by_type).length}
          detail={Object.entries(data.by_type)
            .map(([k, v]) => `${k}: ${v}`)
            .join(", ")}
        />
      </div>

      {/* Activity sparkline */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
        <h3 className="text-sm font-medium text-zinc-400 mb-3">
          Activity (chunks per day)
        </h3>
        <Sparkline data={activityCounts} width={600} height={60} />
      </div>

      {/* Storage growth */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
        <h3 className="text-sm font-medium text-zinc-400 mb-3">
          Storage Growth (cumulative)
        </h3>
        <Sparkline
          data={cumulativeCounts}
          width={600}
          height={60}
          color="#3b82f6"
        />
      </div>

      {/* Memory density heatmap */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
        <h3 className="text-sm font-medium text-zinc-400 mb-3">
          Memory Density (last 12 weeks)
        </h3>
        <ActivityHeatmap data={data.activity} />
      </div>

      {/* Type distribution */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
        <h3 className="text-sm font-medium text-zinc-400 mb-3">
          Content Types
        </h3>
        <SegmentBar
          segments={[
            {
              label: "Conversation",
              value: data.by_type["conversation"] || 0,
              color: "#3b82f6",
            },
            {
              label: "Document",
              value: data.by_type["document"] || 0,
              color: "#f59e0b",
            },
            {
              label: "Knowledge",
              value: data.by_type["knowledge"] || 0,
              color: "#10b981",
            },
          ]}
        />
      </div>

      {/* Source distribution */}
      {Object.keys(data.by_source).length > 0 && (
        <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
          <h3 className="text-sm font-medium text-zinc-400 mb-3">
            Sources (integration)
          </h3>
          <SegmentBar
            segments={Object.entries(data.by_source).map(([k, v], i) => ({
              label: k,
              value: v,
              color: ["#8b5cf6", "#ec4899", "#06b6d4", "#f97316", "#84cc16"][
                i % 5
              ],
            }))}
          />
        </div>
      )}

      {/* Sync status */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
        <h3 className="text-sm font-medium text-zinc-400 mb-3">
          Sync Status
        </h3>
        <SyncBar
          synced={data.sync_status.synced}
          local={data.sync_status.local}
        />
      </div>
    </div>
  );
}
