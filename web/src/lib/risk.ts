// Simple dependency model for blast-radius visualization.
// This is intentionally small and editable; production would come from config/service discovery.

export const ZONE_DEPS: Record<string, string[]> = {
  // zone -> dependencies it requires
  "zone-na": ["zone-eu"],
  "zone-sa": ["zone-na"],
  "zone-eu": ["zone-uk"],
  "zone-uk": [],
  "zone-af": ["zone-eu"],
  "zone-me": ["zone-eu"],
  "zone-in": ["zone-me"],
  "zone-cn": ["zone-in"],
  "zone-ap": ["zone-cn"],
  "zone-au": ["zone-ap"],
};

export function blastRadius(rootZoneId: string): string[] {
  // reverse dependency traversal: all zones that depend (directly or indirectly) on root
  const reverse: Record<string, string[]> = {};
  for (const [z, deps] of Object.entries(ZONE_DEPS)) {
    for (const d of deps) {
      reverse[d] = reverse[d] || [];
      reverse[d].push(z);
    }
  }

  const seen = new Set<string>();
  const q: string[] = [rootZoneId];
  seen.add(rootZoneId);

  while (q.length) {
    const cur = q.shift()!;
    for (const nxt of reverse[cur] || []) {
      if (!seen.has(nxt)) {
        seen.add(nxt);
        q.push(nxt);
      }
    }
  }

  return Array.from(seen);
}

export type ZoneStatus = "OK" | "DEGRADED" | "DOWN";

export type RecommendedControls = {
  writes_blocked: boolean;
  cross_zone_throttle: number;
  spool_enabled: boolean;
  rationale: string;
};

export function recommendedControlsFor(status: ZoneStatus): RecommendedControls {
  if (status === "DOWN") {
    return {
      writes_blocked: true,
      cross_zone_throttle: 0,
      spool_enabled: true,
      rationale: "Containment: block writes and cross-zone flow; spool to replay after recovery.",
    };
  }
  if (status === "DEGRADED") {
    return {
      writes_blocked: false,
      cross_zone_throttle: 50,
      spool_enabled: true,
      rationale: "Stabilization: reduce cross-zone load; spool bursty writes while you triage.",
    };
  }
  return {
    writes_blocked: false,
    cross_zone_throttle: 100,
    spool_enabled: false,
    rationale: "Normal: full flow, no spooling.",
  };
}
