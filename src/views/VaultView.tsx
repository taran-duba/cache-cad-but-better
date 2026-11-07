import { Fragment, useEffect, useState } from "react";
import { ago, api, CheckOutReport, editorsText, Entry, errorText, parseEmails, Status, who } from "../api";
import { Skeleton, Spinner } from "../Spinner";
import type { ToastApi } from "../Toasts";

const KIND_ICON: Record<string, string> = { part: "▣", assembly: "◈", drawing: "▤", other: "▢" };

const STATUS_LABEL: Record<Entry["status"], [string, string]> = {
  synced: ["Up to date", "pill-ok"],
  outdated: ["Update waiting", "pill-info"],
  not_synced: ["Downloading…", "pill-dim"],
  modified: ["Edited", "pill-warn"],
  pending: ["Being pushed", "pill-info"],
};

interface Crumb {
  id: string | null;
  name: string;
  /** The folder's name on this PC (differs when Windows doesn't allow the Drive name). */
  localName: string;
}

/** One toast for any check-out: what was checked out, what a teammate has, what failed. */
function checkOutMessage(r: CheckOutReport, lead: string): [string, boolean] {
  let msg = lead;
  if (r.depsCheckedOut) msg += `, including ${r.depsCheckedOut} part(s) your assemblies use`;
  msg += ".";
  if (r.notYours.length) {
    const shown = r.notYours.slice(0, 5).join(", ");
    msg += ` Left ${r.notYours.length} file(s) owned by other areas read-only: ${shown}${r.notYours.length > 5 ? "…" : ""}.`;
  }
  if (r.lockedByOthers.length) {
    const shown = r.lockedByOthers.slice(0, 5).join(", ");
    msg += ` Already checked out by a teammate: ${shown}${r.lockedByOthers.length > 5 ? ` and ${r.lockedByOthers.length - 5} more` : ""}.`;
  }
  if (r.failed) msg += ` ${r.failed} failed (${r.firstError}).`;
  return [msg, r.failed > 0 || r.lockedByOthers.length > 0];
}

export function VaultView({
  revision,
  onChanged,
  toasts,
  syncing,
  status,
}: {
  status: Status;
  revision: number;
  onChanged: () => void;
  toasts: ToastApi;
  /** A sync pass is running (so an empty list may just mean it hasn't arrived yet). */
  syncing: boolean;
}) {
  const [path, setPath] = useState<Crumb[]>([{ id: null, name: "CAD folder", localName: "" }]);
  const [renaming, setRenaming] = useState<{ id: string; value: string } | null>(null);
  const [rows, setRows] = useState<Entry[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [helpFor, setHelpFor] = useState<string | null>(null);
  /** CADmin editing who may edit a folder or file. */
  const [areaEdit, setAreaEdit] = useState<{ id: string; value: string } | null>(null);
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

  const checkOut = (e: Entry, withParts: boolean) =>
    act(e.id, async () => {
      if (withParts) {
        const r = await api.checkOutWithRefs(e.id);
        const [msg, warn] = checkOutMessage(r, `Checked out ${e.name}`);
        (warn ? toasts.warn : toasts.info)(msg);
      } else {
        await api.checkOut(e.id);
        toasts.info(`Checked out ${e.name} by itself. The parts and subassemblies it uses stay with their owners.`);
      }
    });

  const saveArea = (e: Entry, editors: string[] | null) =>
    act(
      e.id,
      async () => {
        await api.setArea(e.id, editors);
        setAreaEdit(null);
      },
      editors === null
        ? `${e.name} now follows the folder above it.`
        : `${e.name}: ${editorsText(editors)} can edit it now (and CADmins).`,
    );

  const canAssign = status.rolesOn && status.isAdmin;
  const cols = status.rolesOn ? 6 : 5;

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
        <button
          className="btn btn-small"
          disabled={busy !== null || rows === null || rows.length === 0}
          title="Check out every file in this folder (and subfolders) that nobody else has and you may edit"
          onClick={() =>
            act("select-all", async () => {
              const r = await api.checkOutAll(current.id);
              const [msg, warn] = checkOutMessage(r, `Checked out ${r.checkedOut} file(s)`);
              (warn ? toasts.warn : toasts.info)(msg);
            })
          }
        >
          Check out all
        </button>
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
              The first time, Permafrost reads every file name in it. Big folders can take a minute; after that it only
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
              <th>Name</th>
              <th>Status</th>
              {status.rolesOn && <th title="Who may edit it (CADmins can edit everything)">Editors</th>}
              <th>Checked out</th>
              <th>Last change</th>
              <th className="col-actions" />
            </tr>
          </thead>
          <tbody key={current.id ?? "root"}>
            {shown.map((e) => (
              <Fragment key={e.id}>
              <tr className={busy === e.id ? "busy" : ""}>
                <td>
                  {e.isFolder ? (
                    <button
                      className="link name"
                      onClick={() => setPath([...path, { id: e.id, name: e.name, localName: e.localName ?? e.name }])}
                    >
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
                  {!e.isFolder && e.status === "modified" && !e.mine ? (
                    <span
                      className="pill pill-bad"
                      title="These edits can't be pushed because you don't have the file checked out (maybe a teammate took over the lock). Use File › Save As in SolidWorks to keep them under a new name."
                    >
                      Edited, not checked out
                    </span>
                  ) : (
                    !e.isFolder && <span className={`pill ${STATUS_LABEL[e.status][1]}`}>{STATUS_LABEL[e.status][0]}</span>
                  )}
                </td>
                {status.rolesOn && (
                  <td className="small">
                    <span
                      className={e.canEdit ? "muted" : ""}
                      title={
                        e.area
                          ? `Set on ${e.area.path || "the whole CAD folder"}${e.area.id === e.id ? " (this item)" : ""}`
                          : "Not in any area: anyone can edit it"
                      }
                    >
                      {e.area ? editorsText(e.area.editors) : "Anyone"}
                    </span>
                    {!e.canEdit && <span className="pill pill-dim"> View only</span>}
                    {e.area?.id === e.id && <span className="pill pill-info"> Area</span>}
                  </td>
                )}
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
                        🔒 {who(e.lock.owner, e.lock.owner_name)} · {ago(e.lock.locked_at)}
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
                  {!e.isFolder && e.status !== "not_synced" && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small"
                      disabled={busy !== null}
                      title="Open in SolidWorks (or whatever app is set for this file type)"
                      onClick={() => api.openFile(e.id).catch((err) => toasts.error(errorText(err)))}
                    >
                      Open
                    </button>
                  )}
                  {!e.isFolder && !e.lock && e.canEdit && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small"
                      disabled={busy !== null}
                      title={
                        e.kind === "assembly"
                          ? "Just this assembly: its mates, positions and which parts it uses. Parts and subassemblies stay with whoever owns them."
                          : undefined
                      }
                      onClick={() => checkOut(e, e.kind !== "assembly")}
                    >
                      Check out
                    </button>
                  )}
                  {!e.isFolder && !e.lock && e.canEdit && e.kind === "assembly" && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small"
                      disabled={busy !== null}
                      title="This assembly plus every part and subassembly it uses that you're allowed to edit"
                      onClick={() => checkOut(e, true)}
                    >
                      + parts
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
                  {canAssign && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small btn-ghost"
                      disabled={busy !== null}
                      title="Choose who may edit this (CADmins always can)"
                      onClick={() =>
                        setAreaEdit(
                          areaEdit?.id === e.id
                            ? null
                            : { id: e.id, value: e.area?.id === e.id ? e.area.editors.join(", ") : "" },
                        )
                      }
                    >
                      Editors
                    </button>
                  )}
                  {e.canEdit && (e.isFolder || !e.lock || e.mine) && confirming?.id !== e.id && (
                    <button
                      className="btn btn-small btn-ghost"
                      disabled={busy !== null}
                      title="Rename or move it. Assemblies and drawings that use it are updated for everyone."
                      onClick={() =>
                        setRenaming(
                          renaming?.id === e.id
                            ? null
                            : { id: e.id, value: [...path.slice(1).map((c) => c.localName), e.localName ?? e.name].join("/") },
                        )
                      }
                    >
                      Rename
                    </button>
                  )}
                  {!e.isFolder && e.canEdit && (!e.lock || e.mine) && confirming?.id !== e.id && (
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
              {renaming?.id === e.id && (
                <tr className="help-row">
                  <td colSpan={cols}>
                    <form
                      className="rename"
                      onSubmit={(ev) => {
                        ev.preventDefault();
                        const target = renaming.value.trim();
                        act(e.id, async () => {
                          const r = await api.renameItem(e.id, target);
                          setRenaming(null);
                          toasts.info(
                            `${e.name} is now ${r.newPath}.` +
                              (r.updated.length ? ` Updated and pushed ${r.updated.length} assembly/drawing file(s) that use it.` : ""),
                          );
                        });
                      }}
                    >
                      <label className="small">
                        New name or location, inside the CAD folder (use / for folders):
                        <input
                          className="search rename-input"
                          value={renaming.value}
                          autoFocus
                          onChange={(ev) => setRenaming({ id: e.id, value: ev.target.value })}
                          aria-label={`New path for ${e.name}`}
                        />
                      </label>
                      <button className="btn btn-small" type="submit" disabled={busy !== null || !renaming.value.trim()}>
                        Rename / move
                      </button>
                      <button className="btn btn-small btn-ghost" type="button" onClick={() => setRenaming(null)}>
                        Cancel
                      </button>
                      <p className="muted small">
                        Permafrost checks out every assembly and drawing that uses {e.isFolder ? "the files in it" : "it"}, points
                        them at the new location through SolidWorks, and pushes everything together, so nothing breaks for anyone.
                        Close the files in SolidWorks first. Don't rename CAD files in Explorer or on the Drive website.
                      </p>
                    </form>
                  </td>
                </tr>
              )}
              {areaEdit?.id === e.id && (
                <tr className="help-row">
                  <td colSpan={cols}>
                    <form
                      className="rename"
                      onSubmit={(ev) => {
                        ev.preventDefault();
                        saveArea(e, parseEmails(areaEdit.value));
                      }}
                    >
                      <label className="small">
                        Who may edit {e.isFolder ? "everything in" : ""} {e.name}? Google account emails, separated by
                        commas. Leave empty for CADmins only.
                        <input
                          className="search rename-input"
                          value={areaEdit.value}
                          autoFocus
                          placeholder="alex@team.org, blair@team.org"
                          onChange={(ev) => setAreaEdit({ id: e.id, value: ev.target.value })}
                          aria-label={`Editors of ${e.name}`}
                        />
                      </label>
                      <button className="btn btn-small" type="submit" disabled={busy !== null}>
                        Save
                      </button>
                      {e.area?.id === e.id && (
                        <button
                          className="btn btn-small btn-ghost"
                          type="button"
                          disabled={busy !== null}
                          onClick={() => saveArea(e, null)}
                        >
                          Remove (follow the folder above)
                        </button>
                      )}
                      <button className="btn btn-small btn-ghost" type="button" onClick={() => setAreaEdit(null)}>
                        Cancel
                      </button>
                      <p className="muted small">
                        Everyone still gets every file, read-only. Only these people (and CADmins) can check{" "}
                        {e.isFolder ? "files in it" : "it"} out. A folder or file inside can have its own editors, and the
                        nearest one wins. Currently: {e.area ? `${editorsText(e.area.editors)} (set on ${e.area.path || "the CAD folder"})` : "anyone"}.
                      </p>
                    </form>
                  </td>
                </tr>
              )}
              {helpFor === e.id && e.lock && !e.mine && (
                <tr className="help-row">
                  <td colSpan={cols}>
                    <LockHelp
                      entry={e}
                      busy={busy !== null}
                      canTakeOver={e.canEdit}
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
  canTakeOver,
  onClose,
  onTakeOver,
  toasts,
}: {
  entry: Entry;
  busy: boolean;
  canTakeOver: boolean;
  onClose: () => void;
  onTakeOver: () => void;
  toasts: ToastApi;
}) {
  const [sure, setSure] = useState(false);
  const lock = entry.lock!;
  const name = who(lock.owner, lock.owner_name);
  const message = `Hey ${name}, can you push your changes to ${entry.name} in Permafrost (or Undo check-out if you didn't change it)? I need to edit it.`;

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
          <strong>Ask {name} to push their changes.</strong> If they didn't change anything, they can click{" "}
          <em>Undo check-out</em>. When they do, the lock disappears here within about 15 seconds.{" "}
          <button className="link" onClick={copy}>
            Copy a message to send them
          </button>
        </li>
        {!canTakeOver ? (
          <li>
            <strong>Can't reach them?</strong> Ask a CADmin: only {entry.area ? editorsText(entry.area.editors) : "its editors"}{" "}
            and CADmins can take this file over.
          </li>
        ) : (
        <li>
          <strong>Can't reach them?</strong> (They left the shop, their laptop is off, they graduated…) You can take the
          file over. You'll get the latest pushed version.
          {!sure ? (
            <div className="row">
              <button className="btn btn-small" disabled={busy} onClick={() => setSure(true)}>
                Take it over…
              </button>
            </div>
          ) : (
            <div className="notice notice-warn lock-help-warn">
              Anything {name} changed but didn't push stays only on their computer and won't be in the team's
              version. Their Permafrost keeps it as a separate copy so it isn't lost, but someone has to redo or merge it.
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
        )}
      </ol>
    </div>
  );
}
