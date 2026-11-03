// In-app updates: checks the signed manifest on GitHub Releases (endpoint in
// src-tauri/tauri.conf.json), and installs only when the user asks.
import { useCallback, useEffect, useRef, useState } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { errorText } from "./api";

const FIRST_CHECK_MS = 5_000;
const RECHECK_MS = 6 * 60 * 60 * 1000;

export interface UpdaterState {
  /** Version of a newer release, once one is found. */
  version: string | null;
  notes: string | null;
  checking: boolean;
  installing: boolean;
  /** Download progress 0..1, or null while the size is unknown. */
  progress: number | null;
  /** Checks now. Resolves to "available" | "current"; throws when the check itself fails. */
  checkNow: () => Promise<"available" | "current">;
  /** Downloads, installs, and restarts the app. */
  install: () => Promise<void>;
}

export function useUpdater(enabled: boolean): UpdaterState {
  const update = useRef<Update | null>(null);
  const busy = useRef(false);
  const [version, setVersion] = useState<string | null>(null);
  const [notes, setNotes] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);

  const checkNow = useCallback(async () => {
    if (busy.current) return update.current ? "available" : "current";
    busy.current = true;
    setChecking(true);
    try {
      const found = await check();
      update.current = found;
      setVersion(found?.version ?? null);
      setNotes(found?.body ?? null);
      return found ? "available" : "current";
    } finally {
      busy.current = false;
      setChecking(false);
    }
  }, []);

  // Background checks stay quiet: no release yet, or offline, is not worth a message.
  useEffect(() => {
    if (!enabled) return;
    const quiet = () => void checkNow().catch(() => {});
    const first = setTimeout(quiet, FIRST_CHECK_MS);
    const again = setInterval(quiet, RECHECK_MS);
    return () => {
      clearTimeout(first);
      clearInterval(again);
    };
  }, [enabled, checkNow]);

  const install = useCallback(async () => {
    const u = update.current;
    if (!u || busy.current) return;
    busy.current = true;
    setInstalling(true);
    setProgress(null);
    try {
      let total = 0;
      let got = 0;
      await u.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        else if (e.event === "Progress") {
          got += e.data.chunkLength;
          setProgress(total > 0 ? Math.min(1, got / total) : null);
        }
      });
      await relaunch();
    } catch (e) {
      busy.current = false;
      setInstalling(false);
      throw new Error(`Update failed: ${errorText(e)}`);
    }
  }, []);

  return { version, notes, checking, installing, progress, checkNow, install };
}
