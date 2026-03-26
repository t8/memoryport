import { useEffect, useState, useRef, useCallback } from "react";
import { getGraph, getSession, type GraphData, type GraphNode, type SessionChunk } from "../lib/api";
import { Cosmograph, CosmographRef } from "@cosmograph/react";
import { ArrowLeft, ZoomIn, ZoomOut, Maximize2 } from "lucide-react";

export default function Graph() {
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null);
  const [sessionChunks, setSessionChunks] = useState<SessionChunk[]>([]);
  const cosmographRef = useRef<CosmographRef<GraphNode, { source: string; target: string; weight: number }>>(null);

  useEffect(() => {
    setLoading(true);
    getGraph()
      .then((data) => {
        setGraph(data);
        setLoading(false);
      })
      .catch((e) => {
        setError(e.message);
        setLoading(false);
      });
  }, []);

  const handleNodeClick = useCallback(
    async (node: GraphNode | undefined) => {
      if (!node) {
        setSelectedNode(null);
        setSessionChunks([]);
        return;
      }
      setSelectedNode(node);
      try {
        const data = await getSession(node.id);
        const sorted = [...data.chunks].sort((a, b) => {
          if (a.timestamp !== b.timestamp) return a.timestamp - b.timestamp;
          const roleOrder = (r: string | null) =>
            r === "user" ? 0 : r === "assistant" ? 1 : 2;
          return roleOrder(a.role) - roleOrder(b.role);
        });
        setSessionChunks(sorted);
      } catch {
        setSessionChunks([]);
      }
    },
    []
  );

  if (error) {
    return (
      <div className="p-8">
        <p className="text-red-400">Failed to load graph: {error}</p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="p-8 text-zinc-500">Computing knowledge graph...</div>
    );
  }

  if (!graph || graph.nodes.length === 0) {
    return (
      <div className="p-8">
        <h2 className="text-2xl font-bold tracking-tight">Knowledge Graph</h2>
        <p className="text-zinc-500 text-sm mt-1">
          Not enough sessions to build a graph yet. Keep having conversations!
        </p>
      </div>
    );
  }

  // Color nodes by recency
  const now = Date.now();
  const maxAge = Math.max(
    ...graph.nodes.map((n) => now - n.last_timestamp),
    1
  );

  return (
    <div className="flex h-full">
      {/* Graph view */}
      <div className="flex-1 relative bg-zinc-950">
        <div className="absolute top-4 left-4 z-10">
          <h2 className="text-lg font-bold">Knowledge Graph</h2>
          <p className="text-xs text-zinc-500">
            {graph.nodes.length} sessions · {graph.edges.length} connections
          </p>
        </div>

        {/* Controls */}
        <div className="absolute top-4 right-4 z-10 flex gap-1">
          <button
            onClick={() => cosmographRef.current?.zoomIn()}
            className="p-1.5 bg-zinc-800 hover:bg-zinc-700 rounded transition-colors"
          >
            <ZoomIn size={14} />
          </button>
          <button
            onClick={() => cosmographRef.current?.zoomOut()}
            className="p-1.5 bg-zinc-800 hover:bg-zinc-700 rounded transition-colors"
          >
            <ZoomOut size={14} />
          </button>
          <button
            onClick={() => cosmographRef.current?.fitView()}
            className="p-1.5 bg-zinc-800 hover:bg-zinc-700 rounded transition-colors"
          >
            <Maximize2 size={14} />
          </button>
        </div>

        <Cosmograph
          ref={cosmographRef}
          nodes={graph.nodes}
          links={graph.edges.map((e) => ({
            source: e.source,
            target: e.target,
            weight: e.weight,
          }))}
          nodeColor={(node) => {
            const age = now - node.last_timestamp;
            const ratio = 1 - age / maxAge;
            // Green (recent) → zinc (old)
            const r = Math.round(16 + (1 - ratio) * 80);
            const g = Math.round(185 * ratio + 100 * (1 - ratio));
            const b = Math.round(129 * ratio + 100 * (1 - ratio));
            return `rgb(${r},${g},${b})`;
          }}
          nodeSize={(node) => Math.max(3, Math.min(15, node.chunk_count / 2))}
          linkColor={() => "rgba(113, 113, 122, 0.3)"}
          linkWidth={(link) => link.weight * 2}
          onClick={handleNodeClick}
          backgroundColor="#09090b"
          nodeLabelAccessor={(node) => node.label}
          style={{ width: "100%", height: "100%" }}
        />
      </div>

      {/* Detail panel */}
      {selectedNode && (
        <div className="w-96 border-l border-zinc-800 bg-zinc-900/50 overflow-y-auto">
          <div className="p-4 border-b border-zinc-800">
            <button
              onClick={() => {
                setSelectedNode(null);
                setSessionChunks([]);
              }}
              className="flex items-center gap-1.5 text-xs text-zinc-500 hover:text-zinc-300 mb-2 transition-colors"
            >
              <ArrowLeft size={12} />
              Close
            </button>
            <h3 className="font-medium text-sm">{selectedNode.label}</h3>
            <p className="text-xs text-zinc-500 mt-1">
              {selectedNode.chunk_count} chunks ·{" "}
              {new Date(selectedNode.first_timestamp).toLocaleDateString()}
            </p>
          </div>
          <div className="p-4 space-y-2">
            {sessionChunks.slice(0, 20).map((chunk, i) => (
              <div
                key={i}
                className={`rounded p-2 text-xs ${
                  chunk.role === "assistant"
                    ? "bg-zinc-800/50 text-zinc-400"
                    : "bg-zinc-800 text-zinc-300"
                }`}
              >
                <span
                  className={`font-medium ${
                    chunk.role === "assistant"
                      ? "text-blue-400"
                      : "text-emerald-400"
                  }`}
                >
                  {chunk.role}
                </span>
                <p className="mt-1 line-clamp-3">{chunk.content}</p>
              </div>
            ))}
            {sessionChunks.length > 20 && (
              <p className="text-xs text-zinc-600 text-center">
                +{sessionChunks.length - 20} more chunks
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
