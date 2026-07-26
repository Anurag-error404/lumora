import { useCallback, useEffect, useState, type Dispatch, type SetStateAction } from "react";
import {
  api,
  type LockedAlbum,
  type LockedAsset,
  type VaultStatus,
  type VaultSummary,
} from "../lib/tauri";
import type { View } from "../types/app";

/**
 * State + actions for the privacy vault ("Locked folder"). Tracks the list of
 * vaults, whether one is unlocked, the locked folders/items at the current
 * level, and lazily-decrypted thumbnails (kept only in memory as base64 data
 * URLs).
 */
export function useVault({
  view,
  setError,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [vaults, setVaults] = useState<VaultSummary[]>([]);
  const [lockedAlbums, setLockedAlbums] = useState<LockedAlbum[]>([]);
  const [lockedAssets, setLockedAssets] = useState<LockedAsset[]>([]);
  const [thumbs, setThumbs] = useState<Record<string, string>>({});
  /** null = vault list / top level; otherwise the locked folder being viewed. */
  const [openAlbumId, setOpenAlbumId] = useState<string | null>(null);
  /** When set, show unlock UI for this vault instead of the list. */
  const [pendingUnlockId, setPendingUnlockId] = useState<string | null>(null);
  /** When true, show the create-vault form even if vaults already exist. */
  const [creating, setCreating] = useState(false);
  /** When true and a vault is unlocked, show that vault's contents instead of the list. */
  const [browsingContents, setBrowsingContents] = useState(false);

  const refreshStatus = useCallback(async () => {
    try {
      const [nextStatus, nextVaults] = await Promise.all([
        api.vaultStatus(),
        api.listVaults(),
      ]);
      setStatus(nextStatus);
      setVaults(nextVaults);
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  const refreshLocked = useCallback(async () => {
    try {
      const albums = await api.listLockedAlbums();
      // The open folder may have just been emptied, in which case the backend
      // has pruned it and we fall back to the top level.
      const current =
        openAlbumId && albums.some((a) => a.id === openAlbumId) ? openAlbumId : null;
      if (current !== openAlbumId) setOpenAlbumId(current);
      const items = await api.listLockedAssets(current);
      setLockedAlbums(albums);
      setLockedAssets(items);
    } catch (e) {
      setLockedAlbums([]);
      setLockedAssets([]);
      setError(String(e));
    }
  }, [openAlbumId, setError]);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  // Load contents whenever a vault is unlocked on the Locked view.
  useEffect(() => {
    if (view === "locked" && status?.unlocked) {
      void refreshLocked();
    }
    if (!status?.unlocked) {
      setLockedAlbums([]);
      setLockedAssets([]);
      setThumbs({});
      setOpenAlbumId(null);
    }
  }, [view, status?.unlocked, refreshLocked]);

  // Decrypt thumbnails for any newly listed items (in-memory only).
  useEffect(() => {
    let cancelled = false;
    (async () => {
      for (const item of lockedAssets) {
        if (!item.hasThumb || thumbs[item.id]) continue;
        try {
          const url = await api.vaultThumb(item.id);
          if (!cancelled && url) {
            setThumbs((prev) => ({ ...prev, [item.id]: url }));
          }
        } catch {
          // Ignore individual thumbnail failures; tile shows a placeholder.
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [lockedAssets, thumbs]);

  /** Returns the one-time recovery code, which is never retrievable again. */
  const setup = useCallback(
    async (name: string, vaultPath: string, password: string): Promise<string> => {
      const result = await api.setupVault(name, vaultPath, password);
      // Deliberately keep the pre-setup status until the recovery-code screen
      // is acknowledged; otherwise this component would unmount and the
      // one-time code would disappear before it could be saved.
      return result.recoveryCode;
    },
    [],
  );

  const unlock = useCallback(
    async (vaultId: string, password: string) => {
      const next = await api.unlockVault(vaultId, password);
      setStatus(next);
      setPendingUnlockId(null);
      setCreating(false);
      setBrowsingContents(true);
      await refreshStatus();
      await refreshLocked();
    },
    [refreshLocked, refreshStatus],
  );

  const recover = useCallback(
    async (vaultId: string, recoveryCode: string, newPassword: string) => {
      const next = await api.recoverVault(vaultId, recoveryCode, newPassword);
      setStatus(next);
      setPendingUnlockId(null);
      setCreating(false);
      setBrowsingContents(true);
      await refreshStatus();
      await refreshLocked();
    },
    [refreshLocked, refreshStatus],
  );

  const enableRecovery = useCallback(async () => {
    const code = await api.enableVaultRecovery();
    await refreshStatus();
    return code;
  }, [refreshStatus]);

  const lock = useCallback(async () => {
    const next = await api.lockVault();
    setStatus(next);
    setLockedAlbums([]);
    setLockedAssets([]);
    setThumbs({});
    setOpenAlbumId(null);
    setPendingUnlockId(null);
    setBrowsingContents(false);
    await refreshStatus();
  }, [refreshStatus]);

  const openAlbum = useCallback((albumId: string | null) => {
    setOpenAlbumId(albumId);
    setLockedAssets([]);
  }, []);

  const openVault = useCallback(
    (vaultId: string) => {
      const summary = vaults.find((v) => v.id === vaultId);
      if (summary?.unlocked) {
        setPendingUnlockId(null);
        setCreating(false);
        setBrowsingContents(true);
        return;
      }
      setPendingUnlockId(vaultId);
      setCreating(false);
    },
    [vaults],
  );

  const startCreate = useCallback(() => {
    setCreating(true);
    setPendingUnlockId(null);
  }, []);

  const cancelCreate = useCallback(() => {
    setCreating(false);
  }, []);

  const cancelUnlock = useCallback(() => {
    setPendingUnlockId(null);
  }, []);

  const backToVaultList = useCallback(() => {
    setOpenAlbumId(null);
    setBrowsingContents(false);
  }, []);

  const finishSetup = useCallback(async () => {
    setCreating(false);
    setPendingUnlockId(null);
    setBrowsingContents(true);
    await refreshStatus();
    await refreshLocked();
  }, [refreshLocked, refreshStatus]);

  return {
    status,
    vaults,
    lockedAlbums,
    lockedAssets,
    thumbs,
    openAlbumId,
    openAlbum,
    pendingUnlockId,
    creating,
    browsingContents,
    openVault,
    startCreate,
    cancelCreate,
    cancelUnlock,
    backToVaultList,
    finishSetup,
    refreshStatus,
    refreshLocked,
    setup,
    unlock,
    recover,
    enableRecovery,
    lock,
    setLockedAssets,
  };
}
