import { useState, type Dispatch, type SetStateAction } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Icon, IconButton } from "../../components/icons";
import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { formatBytes } from "../../lib/format";
import {
  api,
  type LockedAlbum,
  type LockedAsset,
  type VaultStatus,
  type VaultSummary,
} from "../../lib/tauri";

export function LockedFolderView({
  status,
  vaults,
  lockedAlbums,
  lockedAssets,
  thumbs,
  openAlbumId,
  onOpenAlbum,
  pendingUnlockId,
  creating,
  browsingContents,
  onOpenVault,
  onStartCreate,
  onCancelCreate,
  onCancelUnlock,
  onBackToVaultList,
  onFinishSetup,
  onSetup,
  onUnlock,
  onRecover,
  onEnableRecovery,
  onLock,
  refreshLocked,
  refreshStatus,
  setError,
}: {
  status: VaultStatus | null;
  vaults: VaultSummary[];
  lockedAlbums: LockedAlbum[];
  lockedAssets: LockedAsset[];
  thumbs: Record<string, string>;
  openAlbumId: string | null;
  onOpenAlbum: (albumId: string | null) => void;
  pendingUnlockId: string | null;
  creating: boolean;
  browsingContents: boolean;
  onOpenVault: (vaultId: string) => void;
  onStartCreate: () => void;
  onCancelCreate: () => void;
  onCancelUnlock: () => void;
  onBackToVaultList: () => void;
  onFinishSetup: () => Promise<void>;
  onSetup: (name: string, vaultPath: string, password: string) => Promise<string>;
  onUnlock: (vaultId: string, password: string) => Promise<void>;
  onRecover: (vaultId: string, recoveryCode: string, newPassword: string) => Promise<void>;
  onEnableRecovery: () => Promise<string>;
  onLock: () => Promise<void>;
  refreshLocked: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  if (!status) return <p className="muted">Loading…</p>;

  if (creating || !status.configured) {
    return (
      <VaultSetup
        allowCancel={status.configured}
        onCancel={onCancelCreate}
        onSetup={onSetup}
        onComplete={onFinishSetup}
        setError={setError}
      />
    );
  }

  const pendingVault = pendingUnlockId
    ? vaults.find((v) => v.id === pendingUnlockId)
    : null;

  if (pendingVault && !pendingVault.unlocked) {
    return (
      <VaultUnlock
        vault={pendingVault}
        onUnlock={(password) => onUnlock(pendingVault.id, password)}
        onRecover={(code, password) => onRecover(pendingVault.id, code, password)}
        onBack={onCancelUnlock}
        setError={setError}
      />
    );
  }

  if (status.unlocked && status.vaultId && browsingContents) {
    return (
      <VaultContents
        status={status}
        lockedAlbums={lockedAlbums}
        lockedAssets={lockedAssets}
        thumbs={thumbs}
        openAlbumId={openAlbumId}
        onOpenAlbum={onOpenAlbum}
        onLock={onLock}
        onEnableRecovery={onEnableRecovery}
        onBackToVaults={onBackToVaultList}
        refreshLocked={refreshLocked}
        refreshStatus={refreshStatus}
        setError={setError}
      />
    );
  }

  return (
    <VaultList
      vaults={vaults}
      totalLocked={status.totalLockedCount}
      onOpen={onOpenVault}
      onCreate={onStartCreate}
    />
  );
}

function VaultList({
  vaults,
  totalLocked,
  onOpen,
  onCreate,
}: {
  vaults: VaultSummary[];
  totalLocked: number;
  onOpen: (vaultId: string) => void;
  onCreate: () => void;
}) {
  return (
    <div className="vault-contents">
      <PageHeader
        title="Locked folder"
        description="Password-protected vaults. Each vault has its own password and recovery code. Only one can be unlocked at a time."
        actions={
          <button className="secondary" type="button" onClick={onCreate}>
            <Icon name="plus" className="nav-icon" />
            <span>Create vault</span>
          </button>
        }
      />
      <div className="vault-bar">
        <div className="vault-bar-info">
          <Icon name="lock" className="vault-bar-icon" />
          <span>
            {vaults.length} vault{vaults.length === 1 ? "" : "s"}
            {" · "}
            {totalLocked} locked item{totalLocked === 1 ? "" : "s"}
          </span>
        </div>
      </div>
      {vaults.length === 0 ? (
        <EmptyState
          icon="lock"
          title="No vaults yet"
          description="Create a vault to encrypt sensitive photos with a password and recovery code."
        />
      ) : (
        <section className="vault-folder-section">
          <div className="vault-folder-grid">
            {vaults.map((vault) => (
              <article key={vault.id} className="vault-folder-tile">
                <button className="vault-folder-open" onClick={() => onOpen(vault.id)}>
                  <Icon
                    name={vault.unlocked ? "unlock" : "lock"}
                    className="vault-folder-icon"
                  />
                  <span>
                    <strong>{vault.name}</strong>
                    <small>
                      {vault.lockedCount} item{vault.lockedCount === 1 ? "" : "s"}
                      {vault.unlocked ? " · unlocked" : " · locked"}
                    </small>
                    <small className="vault-path">{vault.path}</small>
                  </span>
                </button>
              </article>
            ))}
          </div>
        </section>
      )}
      <VaultCliHelp vaultPath={vaults.length === 1 ? vaults[0]?.path : null} />
    </div>
  );
}

function VaultSetup({
  onSetup,
  onComplete,
  onCancel,
  allowCancel,
  setError,
}: {
  onSetup: (name: string, vaultPath: string, password: string) => Promise<string>;
  onComplete: () => Promise<void>;
  onCancel?: () => void;
  allowCancel?: boolean;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [name, setName] = useState("");
  const [folder, setFolder] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [recoveryCode, setRecoveryCode] = useState<string | null>(null);
  const [confirmedSaved, setConfirmedSaved] = useState(false);

  async function chooseFolder() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose where to keep the vault contents",
    });
    if (typeof picked === "string") setFolder(picked);
  }

  async function submit() {
    if (!name.trim()) {
      setError("Give this vault a name");
      return;
    }
    if (!folder) {
      setError("Choose a destination folder for the vault");
      return;
    }
    if (password.length < 4) {
      setError("Password must be at least 4 characters");
      return;
    }
    if (password !== confirm) {
      setError("Passwords do not match");
      return;
    }
    setBusy(true);
    try {
      const code = await onSetup(name.trim(), folder, password);
      setError(null);
      setRecoveryCode(code);
      setPassword("");
      setConfirm("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  if (recoveryCode) {
    return (
      <div className="vault-panel vault-recovery-panel">
        <div className="vault-hero">
          <Icon name="lock" className="vault-hero-icon" />
          <h2>Save your recovery code</h2>
          <p className="muted">
            This code is the only way to regain access if you forget your password.
            It will never be shown again. Store it somewhere separate from this Mac.
          </p>
        </div>
        <div className="vault-recovery-code" aria-label="Vault recovery code">
          {recoveryCode}
        </div>
        <button
          className="secondary vault-copy-code"
          onClick={() => {
            void navigator.clipboard.writeText(recoveryCode);
            setError("Recovery code copied. Store it somewhere safe.");
          }}
        >
          <Icon name="copy" className="nav-icon" /> Copy recovery code
        </button>
        <label className="vault-recovery-confirm">
          <input
            type="checkbox"
            checked={confirmedSaved}
            onChange={(event) => setConfirmedSaved(event.target.checked)}
          />
          <span>I saved this code somewhere safe</span>
        </label>
        <button
          className="primary vault-submit"
          disabled={!confirmedSaved}
          onClick={() => void onComplete()}
        >
          Continue to Locked folder
        </button>
      </div>
    );
  }

  return (
    <div className="vault-panel">
      <div className="vault-hero">
        <Icon name="lock" className="vault-hero-icon" />
        <h2>Create a vault</h2>
        <p className="muted">
          Choose a name, destination, and password. Files, names, folder names, and
          paths are all encrypted. You’ll also receive a one-time recovery code.
        </p>
      </div>
      <label className="vault-field">
        <span>Vault name</span>
        <input
          value={name}
          autoFocus
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. Family, Work, Personal"
        />
      </label>
      <label className="vault-field">
        <span>Destination folder</span>
        <div className="vault-folder-row">
          <input readOnly value={folder ?? ""} placeholder="No folder selected" onClick={chooseFolder} />
          <button className="secondary" onClick={chooseFolder}>Choose…</button>
        </div>
      </label>
      <label className="vault-field">
        <span>Password</span>
        <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="At least 4 characters" />
      </label>
      <label className="vault-field">
        <span>Confirm password</span>
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") void submit(); }}
          placeholder="Re-enter password"
        />
      </label>
      <button className="primary vault-submit" onClick={submit} disabled={busy}>
        {busy ? "Setting up…" : "Create vault"}
      </button>
      {allowCancel && onCancel && (
        <button className="vault-link-button" onClick={onCancel}>
          Cancel
        </button>
      )}
    </div>
  );
}

function VaultUnlock({
  vault,
  onUnlock,
  onRecover,
  onBack,
  setError,
}: {
  vault: VaultSummary;
  onUnlock: (password: string) => Promise<void>;
  onRecover: (recoveryCode: string, newPassword: string) => Promise<void>;
  onBack: () => void;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [recovering, setRecovering] = useState(false);
  const [recoveryCode, setRecoveryCode] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirm, setConfirm] = useState("");

  async function submit() {
    if (!password) return;
    setBusy(true);
    try {
      await onUnlock(password);
      setError(null);
      setPassword("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function submitRecovery() {
    if (!recoveryCode.trim()) {
      setError("Enter your recovery code");
      return;
    }
    if (newPassword.length < 4) {
      setError("New password must be at least 4 characters");
      return;
    }
    if (newPassword !== confirm) {
      setError("New passwords do not match");
      return;
    }
    setBusy(true);
    try {
      await onRecover(recoveryCode, newPassword);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="vault-unlock-screen">
      <div className="vault-panel">
        <div className="vault-hero">
          <Icon name="lock" className="vault-hero-icon" />
          <h2>{recovering ? `Recover “${vault.name}”` : vault.name}</h2>
          <p className="muted">
            {vault.lockedCount} item{vault.lockedCount === 1 ? "" : "s"} secured
            {" · "}
            {vault.path}
          </p>
        </div>
        {recovering ? (
          <>
            <label className="vault-field">
              <span>Recovery code</span>
              <input value={recoveryCode} autoFocus onChange={(e) => setRecoveryCode(e.target.value)} placeholder="XXXX-XXXX-XXXX-…" autoCapitalize="characters" />
            </label>
            <label className="vault-field">
              <span>New password</span>
              <input type="password" value={newPassword} onChange={(e) => setNewPassword(e.target.value)} placeholder="At least 4 characters" />
            </label>
            <label className="vault-field">
              <span>Confirm new password</span>
              <input
                type="password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") void submitRecovery(); }}
                placeholder="Re-enter new password"
              />
            </label>
            <button className="primary vault-submit" onClick={submitRecovery} disabled={busy}>
              {busy ? "Recovering…" : "Reset password & unlock"}
            </button>
            <button className="vault-link-button" onClick={() => setRecovering(false)}>Back to password</button>
          </>
        ) : (
          <>
            <label className="vault-field">
              <span>Enter password to unlock</span>
              <input
                type="password"
                value={password}
                autoFocus
                onChange={(e) => setPassword(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") void submit(); }}
                placeholder="Password"
              />
            </label>
            <button className="primary vault-submit" onClick={submit} disabled={busy}>
              {busy ? "Unlocking…" : "Unlock"}
            </button>
            {vault.recoveryConfigured && (
              <button className="vault-link-button" onClick={() => setRecovering(true)}>
                Forgot password? Use recovery code
              </button>
            )}
          </>
        )}
        <button className="vault-link-button" onClick={onBack}>
          Back to vaults
        </button>
      </div>
      <VaultCliHelp vaultPath={vault.path} />
    </div>
  );
}

function VaultContents({
  status,
  lockedAlbums,
  lockedAssets,
  thumbs,
  openAlbumId,
  onOpenAlbum,
  onLock,
  onEnableRecovery,
  onBackToVaults,
  refreshLocked,
  refreshStatus,
  setError,
}: {
  status: VaultStatus;
  lockedAlbums: LockedAlbum[];
  lockedAssets: LockedAsset[];
  thumbs: Record<string, string>;
  openAlbumId: string | null;
  onOpenAlbum: (albumId: string | null) => void;
  onLock: () => Promise<void>;
  onEnableRecovery: () => Promise<string>;
  onBackToVaults: () => void;
  refreshLocked: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<{ item: LockedAsset; url: string } | null>(null);
  const [newRecoveryCode, setNewRecoveryCode] = useState<string | null>(null);
  const selectedIds = [...selected];
  const openAlbum = lockedAlbums.find((album) => album.id === openAlbumId);
  const vaultId = status.vaultId!;

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function openPreview(item: LockedAsset) {
    try {
      setPreview({ item, url: await api.vaultMedia(item.id) });
    } catch (e) {
      setError(String(e));
    }
  }

  async function chooseFolderToLock() {
    const picked = await open({ directory: true, multiple: false, title: "Choose a folder to move into the vault" });
    if (typeof picked !== "string") return;
    if (!window.confirm(`Move every photo and video inside this folder into the encrypted vault?\n\n${picked}\n\nThe original media files will be removed.`)) return;
    setBusy(true);
    try {
      const result = await api.lockFolderToVault(picked, vaultId);
      await Promise.all([refreshLocked(), refreshStatus()]);
      const warn = result.errors.length ? ` · ${result.errors.slice(0, 2).join("; ")}` : "";
      setError(`Moved ${result.locked} item(s) into a locked folder${warn}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function moveOut() {
    if (!selectedIds.length) return;
    const dest = await open({ directory: true, multiple: false, title: "Move locked items to…" });
    if (typeof dest !== "string") return;
    setBusy(true);
    try {
      const result = await api.moveOutLockedAssets(selectedIds, dest);
      setSelected(new Set());
      await Promise.all([refreshLocked(), refreshStatus()]);
      const warn = result.errors.length ? ` · ${result.errors.slice(0, 2).join("; ")}` : "";
      setError(`Moved ${result.restored} item(s) out to ${dest}${warn}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function moveOutFolder(album: LockedAlbum) {
    const dest = await open({ directory: true, multiple: false, title: `Move “${album.name}” to…` });
    if (typeof dest !== "string") return;
    setBusy(true);
    try {
      const result = await api.moveOutLockedAlbum(album.id, dest);
      onOpenAlbum(null);
      await Promise.all([refreshLocked(), refreshStatus()]);
      setError(`Restored “${album.name}” with ${result.restored} item(s) to ${dest}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteForever() {
    if (!selectedIds.length) return;
    if (!window.confirm(`Permanently delete ${selectedIds.length} item(s) from this vault?\n\nThis cannot be undone.`)) return;
    setBusy(true);
    try {
      const removed = await api.deleteLockedAssets(selectedIds);
      setSelected(new Set());
      await Promise.all([refreshLocked(), refreshStatus()]);
      setError(`Permanently deleted ${removed} item(s) from the vault`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deleteFolder(album: LockedAlbum) {
    if (!window.confirm(`Permanently delete “${album.name}” and all ${album.itemCount} encrypted item(s)?\n\nThis cannot be undone.`)) return;
    setBusy(true);
    try {
      const removed = await api.deleteLockedAlbum(album.id);
      onOpenAlbum(null);
      await Promise.all([refreshLocked(), refreshStatus()]);
      setError(`Permanently deleted “${album.name}” (${removed} item(s))`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const title = openAlbum
    ? openAlbum.name
    : status.vaultName ?? "Locked folder";

  return (
    <div className="vault-contents">
      <PageHeader
        title={title}
        description={
          openAlbum
            ? "Encrypted items kept together as a group. Move them out to restore files and folder structure."
            : "Password-protected vault. Filenames and contents stay encrypted on disk until you unlock."
        }
      />
      <div className="vault-bar">
        <div className="vault-bar-info">
          {openAlbum ? (
            <button className="vault-back" onClick={() => { onOpenAlbum(null); setSelected(new Set()); }}>
              <Icon name="chevronLeft" className="nav-icon" /> {status.vaultName ?? "Vault"}
            </button>
          ) : (
            <>
              <button className="vault-back" onClick={onBackToVaults}>
                <Icon name="chevronLeft" className="nav-icon" /> All vaults
              </button>
              <Icon name="unlock" className="vault-bar-icon" />
              <span>{status.lockedCount} locked item{status.lockedCount === 1 ? "" : "s"}</span>
              {status.vaultPath && <span className="muted vault-path">· {status.vaultPath}</span>}
            </>
          )}
          {openAlbum && <strong className="vault-open-folder-name">{openAlbum.name}</strong>}
        </div>
        <div className="spacer" />
        {selectedIds.length > 0 && (
          <>
            <span className="selection-count">{selectedIds.length} selected</span>
            <IconButton icon="download" label="Move out to a folder…" onClick={() => void moveOut()} disabled={busy} />
            <IconButton icon="trash" label="Delete permanently" danger onClick={() => void deleteForever()} disabled={busy} />
          </>
        )}
        {!openAlbumId && (
          <>
            {!status.recoveryConfigured && (
              <button
                className="secondary"
                disabled={busy}
                onClick={async () => {
                  setBusy(true);
                  try {
                    setNewRecoveryCode(await onEnableRecovery());
                  } catch (e) {
                    setError(String(e));
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                Enable recovery
              </button>
            )}
            <button className="secondary" onClick={() => void chooseFolderToLock()} disabled={busy}>
              <Icon name="folder" className="nav-icon" /> Add files to vault
            </button>
          </>
        )}
        <button className="secondary" onClick={() => void onLock()}>
          <Icon name="lock" className="nav-icon" /> Lock now
        </button>
      </div>

      {!openAlbumId && status.vaultPath && (
        <VaultCliHelp vaultPath={status.vaultPath} />
      )}

      {!openAlbumId && lockedAlbums.length > 0 && (
        <section className="vault-folder-section">
          <h3>Locked folders</h3>
          <div className="vault-folder-grid">
            {lockedAlbums.map((album) => (
              <article key={album.id} className="vault-folder-tile">
                <button className="vault-folder-open" onClick={() => onOpenAlbum(album.id)}>
                  <Icon name="folder" className="vault-folder-icon" />
                  <span>
                    <strong>{album.name}</strong>
                    <small>{album.itemCount} item{album.itemCount === 1 ? "" : "s"}</small>
                  </span>
                </button>
                <div className="vault-folder-actions">
                  <IconButton icon="download" label={`Move ${album.name} out`} onClick={() => void moveOutFolder(album)} disabled={busy} />
                  <IconButton icon="trash" label={`Delete ${album.name}`} danger onClick={() => void deleteFolder(album)} disabled={busy} />
                </div>
              </article>
            ))}
          </div>
        </section>
      )}

      {lockedAssets.length === 0 && (!lockedAlbums.length || openAlbumId) ? (
        <EmptyState
          icon="lock"
          title={openAlbum ? `“${openAlbum.name}” is empty` : "This vault is empty"}
          description={openAlbum ? "There are no items in this locked folder." : "Select photos in your library, lock an album, or choose “Add files to vault” to secure an entire folder with encrypted names and structure."}
        />
      ) : lockedAssets.length > 0 ? (
        <div className="vault-grid">
          {lockedAssets.map((item) => {
            const isSelected = selected.has(item.id);
            const thumb = thumbs[item.id];
            return (
              <div key={item.id} className={`vault-tile ${isSelected ? "selected" : ""}`}>
                <button type="button" className="vault-tile-media" onClick={() => void openPreview(item)} title={`Preview ${item.fileName}`}>
                  {thumb ? <img src={thumb} alt={item.fileName} loading="lazy" /> : (
                    <div className="vault-tile-fallback"><Icon name={item.mediaType === "video" ? "play" : "lock"} className="vault-tile-fallback-icon" /></div>
                  )}
                  {item.mediaType === "video" && <span className="vault-tile-badge"><Icon name="play" className="vault-tile-badge-icon" /></span>}
                </button>
                <label className="vault-tile-select">
                  <input type="checkbox" checked={isSelected} onChange={() => toggle(item.id)} />
                </label>
                <div className="vault-tile-meta">
                  <span className="vault-tile-name" title={item.relPath}>{item.fileName}</span>
                  {item.sizeBytes != null && <span className="muted">{formatBytes(item.sizeBytes)}</span>}
                </div>
              </div>
            );
          })}
        </div>
      ) : null}

      {preview && (
        <div className="vault-preview-backdrop" onClick={() => setPreview(null)} role="dialog" aria-modal="true">
          <div className="vault-preview" onClick={(e) => e.stopPropagation()}>
            <div className="vault-preview-head">
              <span title={preview.item.fileName}>{preview.item.fileName}</span>
              <IconButton icon="close" label="Close preview" onClick={() => setPreview(null)} />
            </div>
            {preview.item.mediaType === "video" ? (
              <video src={preview.url} controls autoPlay className="vault-preview-media" />
            ) : (
              <img src={preview.url} alt={preview.item.fileName} className="vault-preview-media" />
            )}
          </div>
        </div>
      )}

      {newRecoveryCode && (
        <div className="vault-preview-backdrop" role="dialog" aria-modal="true">
          <div className="vault-recovery-dialog">
            <h2>Save your recovery code</h2>
            <p className="muted">
              This is the only time it will be shown. Store it somewhere separate
              from this Mac.
            </p>
            <div className="vault-recovery-code">{newRecoveryCode}</div>
            <button
              className="secondary"
              onClick={() => void navigator.clipboard.writeText(newRecoveryCode)}
            >
              <Icon name="copy" className="nav-icon" /> Copy code
            </button>
            <button className="primary" onClick={() => setNewRecoveryCode(null)}>
              I saved it
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** Steps and copyable commands for unlocking a vault folder without the app. */
function VaultCliHelp({ vaultPath }: { vaultPath?: string | null }) {
  const path = vaultPath?.trim() || "/path/to/vault";
  const quoted = shellQuote(path);
  const unlockCmd = `lumora-vault unlock --vault ${quoted} --out ~/Desktop/restored`;
  const listCmd = `lumora-vault list --vault ${quoted}`;
  const passwordCmd = `lumora-vault unlock --vault ${quoted} --out ~/Desktop/restored --password 'YOUR_PASSWORD'`;
  const recoveryCmd = `lumora-vault unlock --vault ${quoted} --out ~/Desktop/restored --recovery 'XXXX-XXXX-XXXX-XXXX'`;
  const buildCmd = "cargo build --release --bin lumora-vault";

  return (
    <details className="vault-cli-help">
      <summary>Unlock this vault from the command line</summary>
      <div className="vault-cli-help-body">
        <p>
          Carry the vault folder (the one with <code>vault.json</code> and{" "}
          <code>blobs/</code>) on another machine and restore files with the{" "}
          <code>lumora-vault</code> CLI — no LUMORA install or database needed.
          Encrypted originals stay in the vault; restored files go to{" "}
          <code>--out</code>.
        </p>
        <ol className="vault-cli-steps">
          <li>
            Build the CLI once from the app source:
            <CliCommand command={buildCmd} />
            The binary lands in <code>src-tauri/target/release/lumora-vault</code>.
            Copy it next to the vault folder if you want it on the same drive.
          </li>
          <li>
            List what is inside (prompts for the password if you omit{" "}
            <code>--password</code>):
            <CliCommand command={listCmd} />
          </li>
          <li>
            Decrypt with the vault password (prompts if you leave{" "}
            <code>--password</code> off):
            <CliCommand command={unlockCmd} />
            <CliCommand command={passwordCmd} />
            <p className="muted vault-cli-env">
              You can also set <code>LUMORA_VAULT_PASSWORD</code> instead of typing
              a password.
            </p>
          </li>
          <li>
            Or decrypt with the recovery key if you forgot the password — use the
            dash-separated code shown when the vault was created:
            <CliCommand command={recoveryCmd} />
            <p className="muted vault-cli-env">
              Same as password unlock: files are written to <code>--out</code> and
              the vault folder stays encrypted on disk.
            </p>
          </li>
        </ol>
      </div>
    </details>
  );
}

function CliCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard can fail in some environments; the command is still selectable.
    }
  }

  return (
    <div className="vault-cli-command">
      <code>{command}</code>
      <button type="button" className="vault-cli-copy" onClick={() => void copy()}>
        <Icon name="copy" className="nav-icon" />
        <span>{copied ? "Copied" : "Copy"}</span>
      </button>
    </div>
  );
}

/** Quote a path for a POSIX shell when it contains spaces or special chars. */
function shellQuote(value: string): string {
  if (/^[A-Za-z0-9_./:@%=+-]+$/.test(value)) return value;
  return `"${value.replace(/(["\\$`])/g, "\\$1")}"`;
}
