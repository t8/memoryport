export default function Logo({ className }: { className?: string }) {
  // Exact pixel grid from Figma "Agent Viralcat" with glasses overlay (node 22:17389)
  // Full 26×26 grid, clipped by "Logo-wide 1" container (31.396 × 21.736)

  const pixels: [number, number][] = [
    // Row 6 — ear tips
    [8,6],[9,6],[16,6],[17,6],
    // Row 7 — ears wider
    [7,7],[8,7],[9,7],[16,7],[17,7],[18,7],
    // Row 8 — forehead
    [7,8],[8,8],[9,8],[10,8],[11,8],[12,8],[13,8],[14,8],[15,8],[16,8],[17,8],[18,8],
    // Row 9 — full head
    [6,9],[7,9],[8,9],[9,9],[10,9],[11,9],[12,9],[13,9],[14,9],[15,9],[16,9],[17,9],[18,9],[19,9],
    // Row 10 — glasses top frame (two glint pixels)
    [7,10],[14,10],
    // Row 11 — glasses middle (lenses + bridge)
    [6,11],[8,11],[12,11],[13,11],[15,11],[19,11],
    // Row 12 — glasses bottom frame
    [6,12],[7,12],[11,12],[12,12],[13,12],[14,12],[18,12],[19,12],
    // Row 13 — nose bridge (dark at col 12–13)
    [6,13],[7,13],[8,13],[9,13],[10,13],[11,13],[14,13],[15,13],[16,13],[17,13],[18,13],[19,13],
    // Row 14 — cheeks
    [7,14],[8,14],[9,14],[10,14],[11,14],[12,14],[13,14],[14,14],[15,14],[16,14],[17,14],[18,14],
    // Row 15–17 — lower face / chin
    [9,15],[10,15],[11,15],[12,15],[13,15],[14,15],[15,15],[16,15],
    [9,16],[10,16],[11,16],[12,16],[13,16],[14,16],[15,16],[16,16],
    [9,17],[10,17],[11,17],[12,17],[13,17],[14,17],[15,17],[16,17],
    // Row 18 — whiskers
    [10,18],[15,18],
  ];

  return (
    <svg
      width="31"
      height="22"
      viewBox="4.93 5.25 16.05 11.11"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      style={{ shapeRendering: "crispEdges" }}
    >
      {pixels.map(([x, y], i) => (
        <rect key={i} x={x} y={y} width={1} height={1} fill="#fff4e0" />
      ))}
    </svg>
  );
}
