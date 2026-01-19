import { ZONES } from "../zones";

type Zone = { id: string; name: string; status: "OK" | "DEGRADED" | "DOWN"; updated_at?: string };

type Props = {
  zones: Zone[];
  selectedId?: string;
  highlightIds?: string[];
  onSelect: (id: string) => void;
};

function statusColor(status?: string) {
  const s = (status || "DOWN").toUpperCase();
  if (s === "OK") return "rgba(34,197,94,.9)";
  if (s === "DEGRADED") return "rgba(245,158,11,.92)";
  return "rgba(239,68,68,.92)";
}

export default function ZoneMap(props: Props) {
  const lookup = new Map(props.zones.map(z => [z.id, z]));
  const highlight = new Set(props.highlightIds || []);

  return (
    <svg viewBox="0 0 800 380" style={{ width: "100%", height: "260px", display: "block" }}>
      <defs>
        <linearGradient id="ocean" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="rgba(96,165,250,.14)" />
          <stop offset="1" stopColor="rgba(17,24,39,.18)" />
        </linearGradient>
      </defs>

      <rect x="0" y="0" width="800" height="380" rx="18" fill="url(#ocean)" stroke="rgba(36,48,71,.7)" />

      {/* abstract continents */}
      <path d="M80,120 C140,70 240,80 280,120 C320,160 300,210 240,230 C170,250 100,210 80,170 Z"
            fill="rgba(17,24,39,.42)" stroke="rgba(36,48,71,.55)" />
      <path d="M335,95 C420,60 525,75 560,125 C590,165 560,210 495,222 C430,235 360,205 338,155 Z"
            fill="rgba(17,24,39,.42)" stroke="rgba(36,48,71,.55)" />
      <path d="M560,125 C635,95 715,122 740,168 C760,210 725,250 655,252 C600,252 560,210 560,175 Z"
            fill="rgba(17,24,39,.42)" stroke="rgba(36,48,71,.55)" />
      <path d="M170,245 C225,235 290,255 305,290 C320,330 272,352 220,347 C170,340 140,310 145,280 Z"
            fill="rgba(17,24,39,.42)" stroke="rgba(36,48,71,.55)" />
      <path d="M625,270 C675,255 730,270 745,305 C760,345 715,362 675,357 C636,350 610,320 615,295 Z"
            fill="rgba(17,24,39,.42)" stroke="rgba(36,48,71,.55)" />

      {ZONES.map(meta => {
        const z = lookup.get(meta.id);
        const selected = props.selectedId === meta.id;
        const fill = statusColor(z?.status);
        const isHighlighted = highlight.has(meta.id);

        return (
          <g
            key={meta.id}
            transform={`translate(${meta.x},${meta.y})`}
            style={{ cursor: "pointer" }}
            onClick={() => props.onSelect(meta.id)}
          >
            {/* blast radius highlight ring */}
            {isHighlighted ? (
              <circle r={26} fill="rgba(245,158,11,.10)" stroke="rgba(245,158,11,.65)" strokeWidth="2" />
            ) : null}

            <circle
              r={selected ? 17 : 15}
              fill="rgba(15,23,42,.92)"
              stroke={selected ? "rgba(96,165,250,.9)" : "rgba(36,48,71,.9)"}
              strokeWidth="2"
            />
            <circle r={9} fill={fill} />
            <text x="-7" y="5" fill="rgba(0,0,0,.85)" fontSize="10" fontWeight="800">{meta.n}</text>
          </g>
        );
      })}
    </svg>
  );
}
