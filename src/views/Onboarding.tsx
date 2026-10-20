import { useEffect, useState } from "react";
import { api, errorText, Status } from "../api";
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
          <FolderPicker account={status.account} onChanged={onChanged} toasts={toasts} />
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
        Sign in with the Google account you use for your team's CAD files. You'll pick the folder next.
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

interface Crumb {
  id: string;
  name: string;
  /** "Shared with me" is a list, not a folder you can pick. */
  pickable: boolean;
}

function FolderPicker({ account, onChanged, toasts }: { account: string; onChanged: () => void; toasts: ToastApi }) {
  const [path, setPath] = useState<Crumb[]>([]); // empty = the starting list
  const [items, setItems] = useState<{ id: string; name: string; pickable: boolean; hint?: string }[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [link, setLink] = useState("");
  const here = path[path.length - 1];

  useEffect(() => {
    let cancelled = false;
    setItems(null);
    const load = here
      ? api.browseFolders(here.id).then((fs) => fs.map((f) => ({ id: f.id, name: f.name, pickable: true })))
      : api.browseLocations().then((ls) =>
          ls.map((l) => ({
            id: l.id,
            name: l.name,
            pickable: l.kind !== "shared_with_me",
            hint: l.kind === "shared_drive" ? "Shared drive" : undefined,
          })),
        );
    load
      .then((r) => !cancelled && setItems(r))
      .catch((e) => {
        if (cancelled) return;
        toasts.error(errorText(e));
        setItems([]);
      });
    return () => {
      cancelled = true;
    };
  }, [here?.id]);

  const choose = async (input: string) => {
    setBusy(true);
    try {
      await api.chooseFolder(input);
      onChanged();
    } catch (e) {
      toasts.error(errorText(e));
      setBusy(false);
    }
  };

  return (
    <>
      <h1>Choose your team's CAD folder</h1>
      <p className="muted">
        Signed in as {account}.{" "}
        <button className="link" onClick={() => api.signOut().then(onChanged)}>
          Use a different account
        </button>
      </p>
      <p className="small muted">
        Pick a whole shared drive, or just one folder inside it (or in My Drive, or shared with you). Only that
        folder syncs.
      </p>

      <div className="inline-form">
        <input
          value={link}
          placeholder="Or paste a link to the folder"
          onChange={(e) => setLink(e.target.value)}
          aria-label="Folder link"
        />
        <button className="btn" disabled={busy || link.trim() === ""} onClick={() => choose(link)}>
          Use link
        </button>
      </div>

      <div className="picker-crumbs">
        <button className="crumb" onClick={() => setPath([])} disabled={path.length === 0}>
          Locations
        </button>
        {path.map((c, i) => (
          <span key={c.id}>
            <span className="crumb-sep">/</span>
            <button className="crumb" onClick={() => setPath(path.slice(0, i + 1))} disabled={i === path.length - 1}>
              {c.name}
            </button>
          </span>
        ))}
      </div>

      {here?.pickable && (
        <button className="btn btn-primary picker-use" disabled={busy} onClick={() => choose(here.id)}>
          {busy ? "Opening…" : `Use “${here.name}”`}
        </button>
      )}

      {items === null ? (
        <p className="muted">Loading…</p>
      ) : items.length === 0 ? (
        <p className="muted small">{here ? "No folders in here." : "Nothing found for this account."}</p>
      ) : (
        <ul className="drive-list">
          {items.map((d) => (
            <li key={d.id}>
              <button
                className="drive"
                disabled={busy}
                onClick={() => setPath([...path, { id: d.id, name: d.name, pickable: d.pickable }])}
              >
                <span className="drive-icon" aria-hidden>
                  {here ? "▸" : "▤"}
                </span>
                {d.name}
                {d.hint && <span className="muted small"> · {d.hint}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
