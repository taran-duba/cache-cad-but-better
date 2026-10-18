import { useEffect, useState } from "react";
import { api, AppConfig, errorText, Status } from "../api";
import type { ToastApi } from "../Toasts";

const EMPTY: AppConfig = { oauth: null, driveId: null, root: null, sidecarPath: null, swdmKey: null, pollSeconds: 15 };

/** Accept a full drive URL or a bare id. */
function driveIdFrom(input: string): string {
  const m = input.match(/folders\/([A-Za-z0-9_-]+)/);
  return (m ? m[1] : input).trim();
}

export function SettingsView({
  status,
  onChanged,
  toasts,
}: {
  status: Status | null;
  onChanged: () => void;
  toasts: ToastApi;
}) {
  const [cfg, setCfg] = useState<AppConfig>(EMPTY);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getConfig()
      .then((c) => setCfg({ ...EMPTY, ...c }))
      .catch((e) => toasts.error(errorText(e)));
  }, []);

  const set = <K extends keyof AppConfig>(k: K, v: AppConfig[K]) => setCfg((c) => ({ ...c, [k]: v }));
  const text = (v: string) => (v.trim() === "" ? null : v.trim());

  const run = async (what: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await what();
      toasts.info(done);
      onChanged();
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const save = () => run(() => api.saveConfig(cfg), "Settings saved");
  const signIn = () =>
    run(async () => {
      await api.saveConfig(cfg);
      await api.signIn();
    }, "Signed in");

  return (
    <section className="settings">
      <h2>Settings</h2>
      {status?.signedIn ? (
        <p className="notice">
          Signed in as <strong>{status.me}</strong>
          {status.demo && " (demo data — nothing here touches Google Drive)"}.{" "}
          <button className="link" onClick={() => run(api.signOut, "Signed out")}>
            Sign out
          </button>
        </p>
      ) : (
        <p className="notice">
          Your team lead sets up a Google Cloud OAuth client once (see docs/SETUP.md) and shares the client id and
          secret. Or{" "}
          <button className="link" onClick={() => run(api.startDemo, "Demo vault loaded")} disabled={busy}>
            try the demo
          </button>{" "}
          first.
        </p>
      )}

      <fieldset>
        <legend>Google Drive</legend>
        <label className="field">
          <span>OAuth client id</span>
          <input
            value={cfg.oauth?.client_id ?? ""}
            onChange={(e) =>
              set("oauth", { client_id: e.target.value.trim(), client_secret: cfg.oauth?.client_secret ?? "" })
            }
            placeholder="1234-abc.apps.googleusercontent.com"
          />
        </label>
        <label className="field">
          <span>OAuth client secret</span>
          <input
            type="password"
            value={cfg.oauth?.client_secret ?? ""}
            onChange={(e) => set("oauth", { client_id: cfg.oauth?.client_id ?? "", client_secret: e.target.value.trim() })}
          />
        </label>
        <label className="field">
          <span>Shared drive</span>
          <input
            value={cfg.driveId ?? ""}
            onChange={(e) => set("driveId", text(driveIdFrom(e.target.value)))}
            placeholder="Paste the shared drive's URL or id"
          />
        </label>
        <label className="field">
          <span>Vault folder on this PC</span>
          <input value={cfg.root ?? ""} onChange={(e) => set("root", text(e.target.value))} placeholder="C:\CAD\Team Drive" />
          <small className="muted">
            Use the same folder on every computer. SolidWorks remembers full paths, so matching paths keep
            assemblies opening correctly for everyone.
          </small>
        </label>
      </fieldset>

      <fieldset>
        <legend>SolidWorks references</legend>
        <label className="field">
          <span>Document Manager license key</span>
          <input type="password" value={cfg.swdmKey ?? ""} onChange={(e) => set("swdmKey", text(e.target.value))} />
          <small className="muted">
            Request it on the SolidWorks customer portal. It lets cadsync read which parts each assembly uses.
          </small>
        </label>
        <label className="field">
          <span>SwRefs helper (optional)</span>
          <input
            value={cfg.sidecarPath ?? ""}
            onChange={(e) => set("sidecarPath", text(e.target.value))}
            placeholder="Leave empty to use the bundled helper"
          />
        </label>
      </fieldset>

      <fieldset>
        <legend>Sync</legend>
        <label className="field narrow">
          <span>Check for changes every (seconds)</span>
          <input
            type="number"
            min={5}
            max={3600}
            value={cfg.pollSeconds}
            onChange={(e) => set("pollSeconds", Number(e.target.value) || 15)}
          />
        </label>
      </fieldset>

      <div className="row">
        <button className="btn" onClick={save} disabled={busy}>
          Save
        </button>
        {!status?.signedIn && (
          <button className="btn btn-primary" onClick={signIn} disabled={busy || !cfg.oauth?.client_id || !cfg.driveId}>
            {busy ? "Waiting for browser…" : "Save and sign in with Google"}
          </button>
        )}
      </div>
    </section>
  );
}
