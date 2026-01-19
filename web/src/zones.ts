export type ZoneMeta = { id: string; n: number; label: string; x: number; y: number };

// IMPORTANT: ids must match seeded DB zones (see db/migrations/0001_init.sql)
export const ZONES: ZoneMeta[] = [
  { id: "zone-na", n: 3, label: "North America", x: 185, y: 150 },
  { id: "zone-sa", n: 7, label: "South America", x: 255, y: 295 },
  { id: "zone-eu", n: 2, label: "Europe", x: 420, y: 145 },
  { id: "zone-uk", n: 4, label: "United Kingdom", x: 390, y: 128 },
  { id: "zone-af", n: 10, label: "Africa", x: 450, y: 245 },
  { id: "zone-me", n: 1, label: "Middle East", x: 505, y: 195 },
  { id: "zone-in", n: 8, label: "India", x: 575, y: 185 },
  { id: "zone-cn", n: 6, label: "China", x: 635, y: 155 },
  { id: "zone-ap", n: 5, label: "Asia Pacific", x: 645, y: 235 },
  { id: "zone-au", n: 9, label: "Australia", x: 705, y: 305 },
];

export function zoneNumber(id: string): number {
  return ZONES.find(z => z.id === id)?.n ?? 0;
}

export function zoneLabel(id: string): string {
  return ZONES.find(z => z.id === id)?.label ?? id;
}
