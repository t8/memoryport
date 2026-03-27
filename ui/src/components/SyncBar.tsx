import SegmentBar from "./SegmentBar";

interface SyncBarProps {
  synced: number;
  local: number;
}

export default function SyncBar({ synced, local }: SyncBarProps) {
  return (
    <SegmentBar
      segments={[
        { label: "Synced to Arweave", value: synced, color: "#84cc16" },
        { label: "Local only", value: local, color: "rgba(255, 244, 224, 0.3)" },
      ]}
    />
  );
}
