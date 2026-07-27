/** Schedule work after first paint / when the browser is idle. */
export function idleDefer(fn: () => void, timeoutMs = 1500): () => void {
  let cancelled = false;
  const run = () => {
    if (!cancelled) fn();
  };

  if (typeof window !== "undefined" && "requestIdleCallback" in window) {
    const handle = window.requestIdleCallback(run, { timeout: timeoutMs });
    return () => {
      cancelled = true;
      window.cancelIdleCallback(handle);
    };
  }

  const handle = globalThis.setTimeout(run, Math.min(timeoutMs, 250));
  return () => {
    cancelled = true;
    globalThis.clearTimeout(handle);
  };
}
