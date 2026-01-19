export function fmtRfc3339(ts?: string): string {
  if (!ts) return "";
  try { return new Date(ts).toLocaleString(); } catch { return ts; }
}

export function fmtUnits(units: number): string {
  const s = Math.max(0, Math.floor(units));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  if (h > 0) return `${h}h ${m}m ${r}s`;
  if (m > 0) return `${m}m ${r}s`;
  return `${r}s`;
}
