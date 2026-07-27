import { useEffect, useRef } from "react";

/**
 * Lock the vault after `autoLockMinutes` of UI inactivity while unlocked.
 * Minutes ≤ 0 disables the timer.
 */
export function useVaultAutoLock({
  unlocked,
  autoLockMinutes,
  onLock,
}: {
  unlocked: boolean;
  autoLockMinutes: number;
  onLock: () => void | Promise<void>;
}) {
  const onLockRef = useRef(onLock);
  onLockRef.current = onLock;

  useEffect(() => {
    if (!unlocked || autoLockMinutes <= 0) return;

    const ms = autoLockMinutes * 60_000;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const arm = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        void onLockRef.current();
      }, ms);
    };

    const onActivity = () => arm();
    const onVisibility = () => {
      if (document.visibilityState === "visible") arm();
    };

    arm();
    window.addEventListener("pointerdown", onActivity, { passive: true });
    window.addEventListener("keydown", onActivity);
    window.addEventListener("mousemove", onActivity, { passive: true });
    window.addEventListener("wheel", onActivity, { passive: true });
    window.addEventListener("touchstart", onActivity, { passive: true });
    document.addEventListener("visibilitychange", onVisibility);
    window.addEventListener("focus", onActivity);

    return () => {
      if (timer) clearTimeout(timer);
      window.removeEventListener("pointerdown", onActivity);
      window.removeEventListener("keydown", onActivity);
      window.removeEventListener("mousemove", onActivity);
      window.removeEventListener("wheel", onActivity);
      window.removeEventListener("touchstart", onActivity);
      document.removeEventListener("visibilitychange", onVisibility);
      window.removeEventListener("focus", onActivity);
    };
  }, [unlocked, autoLockMinutes]);
}
