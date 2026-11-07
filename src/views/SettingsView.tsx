import { useState } from "react";
import { api, errorText, Status } from "../api";
import type { ToastApi } from "../Toasts";
import type { UpdaterState } from "../updater";
import { TeamSettings } from "./TeamSettings";

export function SettingsView({
  status,
  onChanged,
  toasts,
  updater,
  onInstallUpdate,
}: {
  status: Status;
  onChanged: () => void;
  toasts: ToastApi;
  updater: UpdaterState;
  onInstallUpdate: () => void;
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

      {status.projects.length > 0 && (
        <div className="setting-block">
          <div className="setting-label">Projects</div>
          <p className="muted small">
            Each project is its own CAD folder on Drive with its own folder on this PC, for example this season's robot and
            last season's. Switch from the top bar. Removing one only forgets it here: its files stay on this PC and on Drive.
          </p>
          <ul className="plain">
            {status.projects.map((p) => (
              <li key={p.id}>
                <strong>{p.name}</strong> {p.active && <span className="pill pill-info">open</span>}{" "}
                <span className="muted small">{p.root}</span>
                {!p.active && (
                  <button className="btn btn-small" disabled={busy} onClick={() => run(() => api.openProject(p.id))}>
                    Open
                  </button>
                )}
                <button
                  className="btn btn-small btn-ghost"
                  disabled={busy}
                  onClick={() => window.confirm(`Forget ${p.name} on this PC? Its files stay where they are.`) && run(() => api.removeProject(p.id))}
                >
                  Remove
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="setting-row">
        <div>
          <div className="setting-label">{status.projects.length > 0 ? "Add a project" : "CAD folder"}</div>
          <div>{status.projects.length > 0 ? "Open another CAD folder from Google Drive." : status.folderName}</div>
        </div>
        <div className="row">
          {!status.demo && (
            <button className="btn" onClick={() => api.openInDrive(null).catch((e) => toasts.error(errorText(e)))}>
              Open on Drive
            </button>
          )}
          {!status.demo && (
            <button className="btn" onClick={() => run(api.switchDrive)} disabled={busy}>
              {status.projects.length > 0 ? "Add…" : "Change"}
            </button>
          )}
        </div>
      </div>

      <TeamSettings status={status} onChanged={onChanged} toasts={toasts} />

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
          <div className="setting-label">Updates</div>
          <div className="muted small">
            {updater.version
              ? `Version ${updater.version} is ready to install. Permafrost restarts to finish.`
              : "Permafrost checks for new versions in the background."}
          </div>
        </div>
        {updater.version ? (
          <button className="btn btn-primary" onClick={onInstallUpdate} disabled={updater.installing}>
            {updater.installing ? "Updating…" : "Install and restart"}
          </button>
        ) : (
          <button
            className="btn"
            disabled={updater.checking}
            onClick={() =>
              updater
                .checkNow()
                .then((r) => r === "current" && toasts.info("You're on the latest version."))
                .catch((e) => toasts.error(`Couldn't check for updates: ${errorText(e)}`))
            }
          >
            {updater.checking ? "Checking…" : "Check for updates"}
          </button>
        )}
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
