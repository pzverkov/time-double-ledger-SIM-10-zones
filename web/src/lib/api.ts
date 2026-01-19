import { getLS, setLS } from "./storage";

export const LS_API_BASE = "tlsim:apiBase";
const DEFAULT = import.meta.env.VITE_API_BASE || ""; // empty => same-origin (dev proxy)

export function getApiBase(): string {
  return getLS(LS_API_BASE, DEFAULT);
}

export function setApiBase(v: string) {
  setLS(LS_API_BASE, v);
}

type ReqInit = {
  method?: string;
  body?: any;
  headers?: Record<string, string>;
};

export async function api<T = any>(path: string, init?: ReqInit): Promise<T> {
  const base = getApiBase();
  const url = (base ? base.replace(/\/$/, "") : "") + path;

  const headers: Record<string, string> = {
    "content-type": "application/json",
    ...(init?.headers || {}),
  };

  const res = await fetch(url, {
    method: init?.method || "GET",
    headers,
    body: init?.body !== undefined ? JSON.stringify(init.body) : undefined,
  });

  const text = await res.text();
  let json: any = null;
  try { json = text ? JSON.parse(text) : null; } catch { json = { raw: text }; }

  if (!res.ok) {
    const msg =
      (json && (json.error || json.message)) ? (json.error || json.message) :
      (typeof json?.raw === "string" ? json.raw : "") ||
      `HTTP ${res.status}`;
    throw new Error(msg);
  }
  return json as T;
}
