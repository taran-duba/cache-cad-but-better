import { useEffect, useState } from "react";
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
  const [key, setKey] = useState("");
  const [keySaved, setKeySaved] = useState(false);
  const [folder, setFolder] = useState(status.root ?? "");
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getTeamSettings()
      .then((t) => {
        setKey(t.swdmKey ?? "");
        setKeySaved(!!t.swdmKey);
      })
      .catch(() => {});
  }, [status.folderId]);

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
            {status.me}
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

      <div className="setting-block">
        <div className="setting-label">SolidWorks reference checking</div>
        <p className="muted small">
          A Document Manager key (free from the SolidWorks Customer Portal) lets cadsync see which parts each assembly
          uses. Whoever sets it here sets it for the whole team; it's stored in the shared drive.
        </p>
        <div className="inline-form">
          <input
            type="password"
            value={key}
            placeholder={keySaved ? "Saved for the team" : "Paste the Document Manager key"}
            onChange={(e) => setKey(e.target.value)}
            aria-label="Document Manager key"
          />
          <button
            className="btn"
            disabled={busy || key.trim() === ""}
            onClick={() =>
              run(async () => {
                await api.saveTeamSettings({ swdmKey: key.trim() });
                setKeySaved(true);
              }, "Saved for everyone on the team")
            }
          >
            Save
          </button>
        </div>
        {status.refsAvailable ? (
          <p className="small ok-text">Reference checking is on.</p>
        ) : keySaved ? (
          <p className="small muted">Key saved. Reference checking runs on Windows with SolidWorks installed.</p>
        ) : null}
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
