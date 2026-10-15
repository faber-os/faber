/** Back/forward history for a folder browser, as immutable values. */
export interface History {
  back: readonly string[];
  current: string;
  forward: readonly string[];
}

export function startHistory(path: string): History {
  return { back: [], current: path, forward: [] };
}

export function navigate(h: History, path: string): History {
  if (path === h.current) return h;
  return { back: [...h.back, h.current], current: path, forward: [] };
}

export function back(h: History): History {
  const prev = h.back.at(-1);
  if (prev === undefined) return h;
  return { back: h.back.slice(0, -1), current: prev, forward: [h.current, ...h.forward] };
}

export function forward(h: History): History {
  const next = h.forward[0];
  if (next === undefined) return h;
  return { back: [...h.back, h.current], current: next, forward: h.forward.slice(1) };
}

export function canGoBack(h: History): boolean {
  return h.back.length > 0;
}

export function canGoForward(h: History): boolean {
  return h.forward.length > 0;
}
