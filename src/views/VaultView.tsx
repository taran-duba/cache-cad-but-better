import { Fragment, useEffect, useState } from "react";
import { ago, api, Entry, errorText, who } from "../api";
import { Skeleton, Spinner } from "../Spinner";
import type { ToastApi } from "../Toasts";

const KIND_ICON: Record<string, string> = { part: "▣", assembly: "◈", drawing: "▤", other: "▢" };

const STATUS_LABEL: Record<Entry["status"], [string, string]> = {
  synced: ["Up to date", "pill-ok"],
  outdated: ["Update waiting", "pill-info"],
  not_synced: ["Not on this PC", "pill-dim"],
  modified: ["Edited", "pill-warn"],
  pending: ["Being checked in", "pill-info"],
};

interface Crumb {
  id: string | null;
  name: string;
}

export function VaultView({
  revision,
  onChanged,
  toasts,
  syncing,
}: {
  revision: number;
  onChanged: () => void;
  toasts: ToastApi;
  /** A sync pass is running (so an empty list may just mean it hasn't arrived yet). */
  syncing: boolean;
}) {
  const [path, setPath] = useState<Crumb[]>([{ id: null, name: "CAD folder" }]);
  const [rows, setRows] = useState<Entry[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [helpFor, setHelpFor] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<{
    id: string;
    kind: "undo" | "delete" | "in-use";
    detail?: string;
  } | null>(null);
  const current = path[path.length - 1];

  const load = () =>
    api
      .listChildren(current.id)
      .then(setRows)
      .catch((e) => toasts.error(errorText(e)));

  useEffect(() => {
    load();
  }, [current.id, revision]);

  const act = async (id: string, what: () => Promise<unknown>, done?: string) => {
    setConfirming(null);
    setBusy(id);
    try {
      await what();
      if (done) toasts.info(done);
      await load();
      onChanged();
    } catch (e) {
      toasts.error(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const deleteFile = async (e: Entry, force: boolean) => {
    setConfirming(null);
    setBusy(e.id);
    try {
      await api.deleteFromDrive(e.id, force);
      toasts.info(`Deleted ${e.name} for everyone. It's in the shared drive's trash for 30 days.`);
      await load();
      onChanged();
    } catch (err) {
      const text = errorText(err);
      if (!force && text.includes(" is used by ")) {
        setConfirming({ id: e.id, kind: "in-use", detail: text });
      } else {
        toasts.error(text);
      }
    } finally {
      setBusy(null);
    }
  };

  const toggleGet = (e: Entry) => act(e.id, () => api.setSelection(e.id, !e.selected));

  const shown = (rows ?? []).filter((r) => r.name.toLowerCase().includes(filter.toLowerCase()));

  return (
    <section>
      <div className="toolbar">
        <div className="crumbs">
          {path.map((c, i) => (
            <span key={c.id ?? "root"}>
              {i > 0 && <span className="crumb-sep">/</span>}
              <button className="crumb" onClick={() => setPath(path.slice(0, i + 1))} disabled={i === path.length - 1}>
                {c.name}
              </button>
            </span>
          ))}
        </div>
        <input
          className="search"
          placeholder="Filter this folder"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          aria-label="Filter files"
        />
      </div>

      {rows === null ? (
        <Skeleton rows={6} />
      ) : shown.length === 0 && !filter && syncing && path.length === 1 ? (
        <p className="status-banner">
          <Spinner size={18} label="Reading your CAD folder" />
          <span>
            Reading your CAD folder…
            <span className="small">
              The first time, cadsync reads every file name in it. Big folders can take a minute; after that it only
              checks for changes.
            </span>
          </span>
        </p>
      ) : shown.length === 0 ? (
        <p className="muted pad">{filter ? "No matches." : "This folder is empty."}</p>
      ) : (
        <table className="files">
          <thead>
            <tr>
              <th className="col-get" title="Download to this PC">
                Get
              </th>
              <th>Name</th>
              <th>Status</th>
              <th>Checked out</th>
              <th>Last change</th>
              <th className="col-actions" />
            </tr>
          </thead>
          <tbody key={current.id ?? "root"}>
            {shown.map((e) => (
              <Fragment key={e.id}>
              <tr className={busy === e.id ? "busy" : ""}>
                <td className="col-get">
                  <input
                    type="checkbox"
                    checked={e.selected}
                    className={e.selection === null ? "inherited" : ""}
                    title={e.selection === null ? "Inherited from the folder" : "Set on this item"}
                    onChange={() => toggleGet(e)}
                    aria-label={`Get ${e.name}`}
                  />
                </td>
                <td>
                  {e.isFolder ? (
                    <button className="link name" onClick={() => setPath([...path, { id: e.id, name: e.name }])}>
                      <span className="icon">▸</span>
                      {e.name}
                    </button>
                  ) : (
                    <span className="name">
                      <span className={`icon kind-${e.kind}`}>{KIND_ICON[e.kind]}</span>
                      {e.name}
                      {e.localName && e.localName !== e.name && (
                        <span className="muted small" title="Renamed on this PC so Windows accepts it">
                          {" "}
                          → {e.localName}
                        </span>
                      )}
                    </span>
                  )}
                </td>
                <td>
                  {!e.isFolder && e.selected && e.status === "modified" && !e.mine ? (
                    <span
                      className="pill pill-bad"
                      title="These edits can't be checked in because you don't have the file checked out (maybe a teammate took over the lock). Use File › Save As in SolidWorks to keep them under a new name."
                    >
                      Edited, not checked out
                    </span>
                  ) : (
                    !e.isFolder &&
                    e.selected && <span className={`pill ${STATUS_LABEL[e.status][1]}`}>{STATUS_LABEL[e.status][0]}</span>
                  )}
                </td>
                <td>
                  {e.lock &&
                    (e.mine ? (
                      <span className="pill pill-mine">You</span>
                    ) : (
                      <button
                        className={`pill pill-lock pill-button ${isStale(e.lock.locked_at) ? "pill-stale" : ""}`}
                        title={`${e.lock.owner} on ${e.lock.machine} — click for options`}
                        onClick={() => setHelpFor(helpFor === e.id ? null : e.id)}
                      >
                        🔒 {who(e.lock.owner)} · {ago(e.lock.locked_at)}
                        {isStale(e.lock.locked_at) && " · forgotten?"}
                      </button>
                    ))}
                </td>
                <td className="muted small">
                  {!e.isFolder && `${who(e.lastModifier)} · ${ago(e.modifiedTime)}`}
                </td>
                <td className="col-actions">
                  {busy === e.id ? (
                    <span className="working">
                      <Spinner size={12} label="Working" /> Working…
                    </span>
                  ) : (
                    <>
                  {!e.isFolder && !e.lock && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small"
                      disabled={busy !== null}
                      onClick={() => act(e.id, () => api.checkOut(e.id), `Checked out ${e.name}`)}
                    >
                      Check out
                    </button>
                  )}
                  {!e.isFolder && e.mine && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small"
                      disabled={busy !== null}
                      onClick={() =>
                        e.status === "modified"
                          ? setConfirming({ id: e.id, kind: "undo" })
                          : act(e.id, () => api.undoCheckOut(e.id, false), `Released ${e.name}`)
                      }
                    >
                      Undo check-out
                    </button>
                  )}
                  {!e.isFolder && e.lock && !e.mine && (
                    <button
                      className="btn btn-small"
                      disabled={busy !== null}
                      onClick={() => setHelpFor(helpFor === e.id ? null : e.id)}
                    >
                      Need it?
                    </button>
                  )}
                  {!e.isFolder && (!e.lock || e.mine) && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small btn-ghost"
                      disabled={busy !== null}
                      title="Delete for the whole team (goes to the shared drive's trash)"
                      onClick={() => setConfirming({ id: e.id, kind: "delete" })}
                    >
                      Delete
                    </button>
                  )}
                  {confirming?.id === e.id && (confirming.kind === "delete" || confirming.kind === "in-use") && (
                    <span className="confirm">
                      <span className="small">
                        {confirming.kind === "in-use"
                          ? `${confirming.detail}. Deleting it will break those for everyone.`
                          : "Delete for the whole team?"}
                      </span>
                      <button
                        className="btn btn-small btn-danger"
                        onClick={() => deleteFile(e, confirming.kind === "in-use")}
                      >
                        {confirming.kind === "in-use" ? "Delete anyway" : "Delete"}
                      </button>
                      <button className="btn btn-small btn-ghost" onClick={() => setConfirming(null)}>
                        Cancel
                      </button>
                    </span>
                  )}
                  {confirming?.id === e.id && confirming.kind === "undo" && (
                    <span className="confirm">
                      <span className="small">Your edits?</span>
                      <button
                        className="btn btn-small"
                        onClick={() => act(e.id, () => api.undoCheckOut(e.id, false), `Released ${e.name}, edits kept`)}
                      >
                        Keep on this PC
                      </button>
                      <button
                        className="btn btn-small btn-danger"
                        onClick={() => act(e.id, () => api.undoCheckOut(e.id, true), `Released ${e.name}, edits discarded`)}
                      >
                        Throw away
                      </button>
                      <button className="btn btn-small btn-ghost" onClick={() => setConfirming(null)}>
                        Cancel
                      </button>
                    </span>
                  )}
                    </>
                  )}
                </td>
              </tr>
              {helpFor === e.id && e.lock && !e.mine && (
                <tr className="help-row">
                  <td colSpan={6}>
                    <LockHelp
                      entry={e}
                      busy={busy !== null}
                      onClose={() => setHelpFor(null)}
                      onTakeOver={() =>
                        act(
                          e.id,
                          async () => {
                            await api.breakLock(e.id);
                            await api.checkOut(e.id);
                            setHelpFor(null);
                          },
                          `You have ${e.name} checked out now`,
                        )
                      }
                      toasts={toasts}
                    />
                  </td>
                </tr>
              )}
              </Fragment>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}

const DAY_MS = 24 * 3600 * 1000;

function isStale(lockedAt: string) {
  const t = Date.parse(lockedAt);
  return !Number.isNaN(t) && Date.now() - t > DAY_MS;
}

/** What to do when someone else has the file you need. */
function LockHelp({
  entry,
  busy,
  onClose,
  onTakeOver,
  toasts,
}: {
  entry: Entry;
  busy: boolean;
  onClose: () => void;
  onTakeOver: () => void;
  toasts: ToastApi;
}) {
  const [sure, setSure] = useState(false);
  const lock = entry.lock!;
  const name = who(lock.owner);
  const message = `Hey ${name}, can you check in ${entry.name} in cadsync (or Undo check-out if you didn't change it)? I need to edit it.`;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(message);
      toasts.info("Message copied. Paste it to them in your team chat.");
    } catch {
      toasts.warn(message);
    }
  };

  return (
    <div className="lock-help">
      <div className="lock-help-head">
        <strong>
          {name} has {entry.name} checked out
        </strong>
        <span className="muted small">
          {" "}
          on {lock.machine || "their computer"}, since {ago(lock.locked_at)}
        </span>
        <button className="btn btn-small btn-ghost lock-help-close" onClick={onClose} aria-label="Close">
          ✕
        </button>
      </div>
      <ol>
        <li>
          <strong>Ask {name} to check it in.</strong> If they didn't change anything, they can click{" "}
          <em>Undo check-out</em>. When they do, the lock disappears here within about 15 seconds.{" "}
          <button className="link" onClick={copy}>
            Copy a message to send them
          </button>
        </li>
        <li>
          <strong>Can't reach them?</strong> (They left the shop, their laptop is off, they graduated…) You can take the
          file over. You'll get the latest checked-in version.
          {!sure ? (
            <div className="row">
              <button className="btn btn-small" disabled={busy} onClick={() => setSure(true)}>
                Take it over…
              </button>
            </div>
          ) : (
            <div className="notice notice-warn lock-help-warn">
              Anything {name} changed but didn't check in stays only on their computer and won't be in the team's
              version. Their cadsync keeps it as a separate copy so it isn't lost, but someone has to redo or merge it.
              <div className="row">
                <button className="btn btn-small btn-danger" disabled={busy} onClick={onTakeOver}>
                  Yes, take it over
                </button>
                <button className="btn btn-small btn-ghost" onClick={() => setSure(false)}>
                  Cancel
                </button>
              </div>
            </div>
          )}
        </li>
      </ol>
    </div>
  );
}
