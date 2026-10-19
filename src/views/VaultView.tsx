import { useEffect, useState } from "react";
import { ago, api, Entry, errorText, who } from "../api";
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
}: {
  revision: number;
  onChanged: () => void;
  toasts: ToastApi;
}) {
  const [path, setPath] = useState<Crumb[]>([{ id: null, name: "Shared drive" }]);
  const [rows, setRows] = useState<Entry[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [confirming, setConfirming] = useState<{
    id: string;
    kind: "undo" | "break" | "delete" | "in-use";
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
        <p className="muted pad">Loading…</p>
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
          <tbody>
            {shown.map((e) => (
              <tr key={e.id} className={busy === e.id ? "busy" : ""}>
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
                  {!e.isFolder && e.selected && (
                    <span className={`pill ${STATUS_LABEL[e.status][1]}`}>{STATUS_LABEL[e.status][0]}</span>
                  )}
                </td>
                <td>
                  {e.lock &&
                    (e.mine ? (
                      <span className="pill pill-mine">You</span>
                    ) : (
                      <span className="pill pill-lock" title={`${e.lock.owner} on ${e.lock.machine}`}>
                        🔒 {who(e.lock.owner)} · {ago(e.lock.locked_at)}
                      </span>
                    ))}
                </td>
                <td className="muted small">
                  {!e.isFolder && `${who(e.lastModifier)} · ${ago(e.modifiedTime)}`}
                </td>
                <td className="col-actions">
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
                  {!e.isFolder && e.lock && !e.mine && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small btn-ghost"
                      disabled={busy !== null}
                      onClick={() => setConfirming({ id: e.id, kind: "break" })}
                    >
                      Break lock
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
                  {confirming?.id === e.id && confirming.kind === "break" && (
                    <span className="confirm">
                      <span className="small">
                        {who(e.lock?.owner)} can't check in their edits after this.
                      </span>
                      <button
                        className="btn btn-small btn-danger"
                        onClick={() => act(e.id, () => api.breakLock(e.id), "Lock removed")}
                      >
                        Break it
                      </button>
                      <button className="btn btn-small btn-ghost" onClick={() => setConfirming(null)}>
                        Cancel
                      </button>
                    </span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
