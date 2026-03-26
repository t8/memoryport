import { useEffect, useState, useRef, useCallback } from "react";
import {
  getGraph,
  getSession,
  type GraphData,
  type GraphNode,
  type GraphEdge,
  type SessionChunk,
} from "../lib/api";
import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceCenter,
  forceCollide,
  type SimulationNodeDatum,
  type SimulationLinkDatum,
} from "d3-force";
import { scaleLinear } from "d3-scale";
import { ArrowLeft, Bot, User } from "lucide-react";

interface SimNode extends SimulationNodeDatum {
  id: string;
  label: string;
  chunk_count: number;
  first_timestamp: number;
  last_timestamp: number;
}

interface SimLink extends SimulationLinkDatum<SimNode> {
  weight: number;
}

export default function Graph() {
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nodes, setNodes] = useState<SimNode[]>([]);
  const [links, setLinks] = useState<SimLink[]>([]);
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<SimNode | null>(null);
  const [sessionChunks, setSessionChunks] = useState<SessionChunk[]>([]);
  const [sessionCache, setSessionCache] = useState<Record<string, SessionChunk[]>>({});
  const [simulating, setSimulating] = useState(true);
  const [simProgress, setSimProgress] = useState(0);
  const svgRef = useRef<SVGSVGElement>(null);

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

  // Run force simulation when graph data changes, with position caching
  useEffect(() => {
    if (!graph || graph.nodes.length === 0) return;

    // Try to load cached positions from localStorage
    const cacheKey = `memoryport-graph-${graph.nodes.length}`;
    const cachedPositions: Record<string, { x: number; y: number }> = (() => {
      try {
        const cached = localStorage.getItem(cacheKey);
        return cached ? JSON.parse(cached) : {};
      } catch {
        return {};
      }
    })();

    const hasCachedPositions = Object.keys(cachedPositions).length === graph.nodes.length;

    const simNodes: SimNode[] = graph.nodes.map((n) => ({
      ...n,
      x: cachedPositions[n.id]?.x ?? n.x + Math.random() * 50,
      y: cachedPositions[n.id]?.y ?? n.y + Math.random() * 50,
      // If all positions are cached, fix them initially
      ...(hasCachedPositions ? { fx: cachedPositions[n.id]?.x, fy: cachedPositions[n.id]?.y } : {}),
    }));

    const simLinks: SimLink[] = graph.edges.map((e) => ({
      source: e.source,
      target: e.target,
      weight: e.weight,
    }));

    setSimulating(true);
    setSimProgress(0);

    const totalTicks = 300;
    let tickCount = 0;

    const simulation = forceSimulation<SimNode>(simNodes)
      .force(
        "link",
        forceLink<SimNode, SimLink>(simLinks)
          .id((d) => d.id)
          .distance((d) => 100 * (1 - d.weight))
          .strength((d) => d.weight)
      )
      .force("charge", forceManyBody().strength(-200))
      .force("center", forceCenter(400, 300))
      .force("collide", forceCollide(30))
      .alphaDecay(0.02);

    simulation.on("tick", () => {
      tickCount++;
      setNodes([...simNodes]);
      setLinks([...simLinks]);
      setSimProgress(Math.min(100, Math.round((tickCount / totalTicks) * 100)));
    });

    simulation.on("end", () => {
      setSimulating(false);
      setSimProgress(100);
      // Cache settled positions
      const positions: Record<string, { x: number; y: number }> = {};
      simNodes.forEach((n) => {
        if (n.x != null && n.y != null) {
          positions[n.id] = { x: n.x, y: n.y };
        }
      });
      try {
        localStorage.setItem(cacheKey, JSON.stringify(positions));
      } catch {}
    });

    // If all positions were cached, immediately show them and skip simulation
    if (hasCachedPositions) {
      setNodes([...simNodes]);
      setLinks([...graph.edges.map((e) => ({
        source: e.source,
        target: e.target,
        weight: e.weight,
      }))]);
      setSimulating(false);
      setSimProgress(100);
      // Unfix nodes so they can still be dragged later
      simNodes.forEach((n) => { n.fx = undefined; n.fy = undefined; });
      simulation.alpha(0.01).restart(); // tiny nudge to settle links
    }

    return () => {
      simulation.stop();
    };
  }, [graph]);

  const handleNodeClick = useCallback(
    async (node: SimNode) => {
      setSelectedNode(node);
      // Check in-memory cache first
      if (sessionCache[node.id]) {
        setSessionChunks(sessionCache[node.id]);
        return;
      }
      try {
        const data = await getSession(node.id);
        const sorted = [...data.chunks].sort((a, b) => {
          if (a.timestamp !== b.timestamp) return a.timestamp - b.timestamp;
          const roleOrder = (r: string | null) =>
            r === "user" ? 0 : r === "assistant" ? 1 : 2;
          return roleOrder(a.role) - roleOrder(b.role);
        });
        setSessionChunks(sorted);
        // Cache for next click
        setSessionCache((prev) => ({ ...prev, [node.id]: sorted }));
      } catch {
        setSessionChunks([]);
      }
    },
    [sessionCache]
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

  // Color scale by recency
  const now = Date.now();
  const maxAge = Math.max(...graph.nodes.map((n) => now - n.last_timestamp), 1);
  const sizeScale = scaleLinear()
    .domain([1, Math.max(...graph.nodes.map((n) => n.chunk_count))])
    .range([8, 28]);

  return (
    <div className="flex h-full">
      {/* Graph */}
      <div className="flex-1 relative bg-zinc-950 overflow-hidden">
        <div className="absolute top-4 left-4 z-10">
          <h2 className="text-lg font-bold">Knowledge Graph</h2>
          <p className="text-xs text-zinc-500">
            {graph.nodes.length} sessions · {graph.edges.length} connections
          </p>
          {simulating && (
            <div className="mt-2 flex items-center gap-2">
              <div className="w-24 h-1.5 bg-zinc-800 rounded-full overflow-hidden">
                <div
                  className="h-full bg-emerald-500 transition-all duration-300 rounded-full"
                  style={{ width: `${simProgress}%` }}
                />
              </div>
              <span className="text-xs text-zinc-600">
                Arranging {simProgress}%
              </span>
            </div>
          )}
        </div>

        <svg
          ref={svgRef}
          width="100%"
          height="100%"
          viewBox="0 0 800 600"
          className="w-full h-full"
        >
          {/* Edges */}
          {links.map((link, i) => {
            const source = link.source as SimNode;
            const target = link.target as SimNode;
            if (!source.x || !source.y || !target.x || !target.y) return null;
            const isHighlighted =
              hoveredNode === source.id || hoveredNode === target.id;
            return (
              <line
                key={i}
                x1={source.x}
                y1={source.y}
                x2={target.x}
                y2={target.y}
                stroke={isHighlighted ? "rgba(16,185,129,0.5)" : "rgba(113,113,122,0.15)"}
                strokeWidth={isHighlighted ? 2 : 1}
              />
            );
          })}

          {/* Nodes */}
          {nodes.map((node) => {
            if (!node.x || !node.y) return null;
            const age = now - node.last_timestamp;
            const freshness = 1 - age / maxAge;
            const r = Math.round(16 + (1 - freshness) * 80);
            const g = Math.round(185 * freshness + 100 * (1 - freshness));
            const b = Math.round(129 * freshness + 100 * (1 - freshness));
            const color = `rgb(${r},${g},${b})`;
            const radius = sizeScale(node.chunk_count);
            const isHovered = hoveredNode === node.id;
            const isSelected = selectedNode?.id === node.id;

            return (
              <g
                key={node.id}
                onMouseEnter={() => setHoveredNode(node.id)}
                onMouseLeave={() => setHoveredNode(null)}
                onClick={() => handleNodeClick(node)}
                className="cursor-pointer"
              >
                {/* Glow on hover */}
                {(isHovered || isSelected) && (
                  <circle
                    cx={node.x}
                    cy={node.y}
                    r={radius + 6}
                    fill="none"
                    stroke={color}
                    strokeWidth={2}
                    opacity={0.4}
                  />
                )}
                <circle
                  cx={node.x}
                  cy={node.y}
                  r={radius}
                  fill={color}
                  opacity={hoveredNode && !isHovered ? 0.3 : 0.9}
                />
                {/* Label */}
                {(isHovered || isSelected || node.chunk_count > 3) && (
                  <text
                    x={node.x}
                    y={node.y! + radius + 14}
                    textAnchor="middle"
                    fill="#a1a1aa"
                    fontSize={10}
                  >
                    {node.label.length > 25
                      ? node.label.slice(0, 25) + "…"
                      : node.label}
                  </text>
                )}
                {/* Chunk count */}
                <text
                  x={node.x}
                  y={node.y! + 4}
                  textAnchor="middle"
                  fill="white"
                  fontSize={radius > 12 ? 10 : 8}
                  fontWeight="bold"
                >
                  {node.chunk_count}
                </text>
              </g>
            );
          })}
        </svg>
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
                <div className="flex items-center gap-1 mb-1">
                  {chunk.role === "assistant" ? (
                    <Bot size={10} className="text-blue-400" />
                  ) : (
                    <User size={10} className="text-emerald-400" />
                  )}
                  <span
                    className={`font-medium ${
                      chunk.role === "assistant"
                        ? "text-blue-400"
                        : "text-emerald-400"
                    }`}
                  >
                    {chunk.source_model || chunk.role || "unknown"}
                  </span>
                </div>
                <p className="line-clamp-3">{chunk.content}</p>
              </div>
            ))}
            {sessionChunks.length > 20 && (
              <p className="text-xs text-zinc-600 text-center">
                +{sessionChunks.length - 20} more
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
