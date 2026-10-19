import { useEffect, useRef, useState } from "react";
import { api, errorText, SharedDrive, Status } from "../api";
import type { ToastApi } from "../Toasts";

/** Shown until a shared drive is open: sign in with Google, then pick the drive. */
export function Onboarding({
  status,
  onChanged,
  toasts,
}: {
  status: Status;
  onChanged: () => void;
  toasts: ToastApi;
}) {
  return (
    <div className="onboarding">
      <div className="onboarding-card">
        <div className="onboarding-brand">
          <span className="logo" aria-hidden>
            ◆
          </span>{" "}
          cadsync
        </div>
        {status.account ? (
          <DrivePicker account={status.account} onChanged={onChanged} toasts={toasts} />
        ) : (
          <Welcome status={status} onChanged={onChanged} toasts={toasts} />
        )}
      </div>
    </div>
  );
}

function Welcome({ status, onChanged, toasts }: { status: Status; onChanged: () => void; toasts: ToastApi }) {
  const [busy, setBusy] = useState(false);

  const run = async (what: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await what();
      onChanged();
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      {status.signInExpired && (
        <p className="notice notice-warn">
          Your Google sign-in expired or was removed. Sign in again — your files and any unfinished check-in are still
          here.
        </p>
      )}
      <h1>Your team's CAD, in sync.</h1>
      <p className="muted">
        Sign in with the Google account you use for your team's shared drive. You'll pick the drive next.
      </p>
      {status.signInAvailable ? (
        <button className="btn btn-primary btn-big" onClick={() => run(api.signIn)} disabled={busy}>
          {busy ? "Finish signing in in your browser…" : "Sign in with Google"}
        </button>
      ) : (
        <p className="notice notice-warn">
          This copy of cadsync was built without Google sign-in. Whoever builds the app needs to add the team's
          Google client file first (see docs/SETUP.md).
        </p>
      )}
      <p className="small">
        <button className="link" onClick={() => run(api.startDemo)} disabled={busy}>
          Just looking? Try the demo
        </button>
      </p>
    </>
  );
}

function DrivePicker({ account, onChanged, toasts }: { account: string; onChanged: () => void; toasts: ToastApi }) {
  const [drives, setDrives] = useState<SharedDrive[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const autoPicked = useRef(false);

  const choose = async (id: string) => {
    setBusy(id);
    try {
      await api.chooseDrive(id);
      onChanged();
    } catch (e) {
      toasts.error(errorText(e));
      setBusy(null);
    }
  };

  useEffect(() => {
    api
      .listSharedDrives()
      .then((d) => {
        setDrives(d);
        if (d.length === 1 && !autoPicked.current) {
          autoPicked.current = true; // only one drive: no need to ask
          choose(d[0].id);
        }
      })
      .catch((e) => {
        toasts.error(errorText(e));
        setDrives([]);
      });
  }, []);

  return (
    <>
      <h1>Pick your team's shared drive</h1>
      <p className="muted">
        Signed in as {account}.{" "}
        <button className="link" onClick={() => api.signOut().then(onChanged)}>
          Use a different account
        </button>
      </p>
      {drives === null ? (
        <p className="muted">Looking for shared drives…</p>
      ) : drives.length === 0 ? (
        <p className="notice notice-warn">
          This account isn't a member of any shared drive. Ask your CAD lead to add you to the team's shared drive,
          then try again.
        </p>
      ) : (
        <ul className="drive-list">
          {drives.map((d) => (
            <li key={d.id}>
              <button className="drive" onClick={() => choose(d.id)} disabled={busy !== null}>
                <span className="drive-icon" aria-hidden>
                  ▤
                </span>
                {d.name}
                {busy === d.id && <span className="muted small"> opening…</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
