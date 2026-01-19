export function getLS(key: string, fallback: string): string {
  try {
    const v = localStorage.getItem(key);
    return v ?? fallback;
  } catch {
    return fallback;
  }
}

export function setLS(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch {}
}
