import SegmentBar from "./SegmentBar";

interface SyncBarProps {
  synced: number;
  local: number;
}

export default function SyncBar({ synced, local }: SyncBarProps) {
  return (
    <SegmentBar
      segments={[
        { label: "Synced to Arweave", value: synced, color: "#10b981" },
        { label: "Local only", value: local, color: "#71717a" },
      ]}
    />
  );
}
