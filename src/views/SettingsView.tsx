import { useState } from "react";
import { api, errorText, Status } from "../api";
import type { ToastApi } from "../Toasts";

export function SettingsView({
  status,
  onChanged,
  toasts,
}: {
  status: Status;
  onChanged: () => void;
  toasts: ToastApi;
}) {
  const [folder, setFolder] = useState(status.root ?? "");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [busy, setBusy] = useState(false);

  const run = async (what: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    try {
      await what();
      if (done) toasts.info(done);
      onChanged();
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="settings">
      <h2>Settings</h2>

      <div className="setting-row">
        <div>
          <div className="setting-label">Account</div>
          <div>
            {status.accountName || status.me}
            {status.accountName && status.me && <span className="muted"> ({status.me})</span>}
            {status.demo && <span className="muted"> (demo — nothing here touches Google Drive)</span>}
          </div>
        </div>
        <button className="btn" onClick={() => run(api.signOut)} disabled={busy}>
          Sign out
        </button>
      </div>

      <div className="setting-row">
        <div>
          <div className="setting-label">CAD folder</div>
          <div>{status.folderName}</div>
        </div>
        {!status.demo && (
          <button className="btn" onClick={() => run(api.switchDrive)} disabled={busy}>
            Change
          </button>
        )}
      </div>

      <div className="setting-row">
        <div>
          <div className="setting-label">SolidWorks reference checking</div>
          <div className="muted small">
            Permafrost asks SolidWorks which parts each assembly uses, so SolidWorks must be installed on this PC. If it
            isn't open, it starts in the background (the first check can take a minute).
          </div>
          <label className="check">
            <input
              type="checkbox"
              checked={status.refsEnabled}
              disabled={busy}
              onChange={(e) => run(() => api.setRefsEnabled(e.target.checked))}
            />
            Check references with SolidWorks
          </label>
          {status.refsEnabled && !status.refsAvailable && (
            <div className="small muted">Not available here: it runs on Windows with SolidWorks installed.</div>
          )}
        </div>
      </div>

      <div className="setting-row">
        <div>
          <div className="setting-label">Something went wrong?</div>
          <div className="muted small">The log file and any crash reports stay on this computer. Send them along with a bug report.</div>
        </div>
        <button className="btn" onClick={() => api.openLogs().catch((e) => toasts.error(errorText(e)))}>
          Open logs folder
        </button>
      </div>

      <button className="link small" onClick={() => setShowAdvanced(!showAdvanced)}>
        {showAdvanced ? "Hide advanced" : "Advanced"}
      </button>
      {showAdvanced && (
        <div className="setting-block">
          <div className="setting-label">Vault folder on this computer</div>
          <p className="muted small">
            Keep this the same on every computer — SolidWorks remembers full paths, so matching folders keep
            assemblies opening for everyone.
          </p>
          <div className="inline-form">
            <input value={folder} onChange={(e) => setFolder(e.target.value)} aria-label="Vault folder" />
            <button
              className="btn"
              disabled={busy || status.demo || folder.trim() === "" || folder === status.root}
              onClick={() => run(() => api.setVaultFolder(folder.trim()), "Vault folder changed")}
            >
              Change
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
