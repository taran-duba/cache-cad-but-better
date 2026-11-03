import { useEffect, useState } from "react";
import type { ReleaseInfo } from "../../api/release";

export const REPO_URL = "https://github.com/taran-duba/permafrost";
export const RELEASES_URL = `${REPO_URL}/releases`;
export const SETUP_URL = `${REPO_URL}/blob/main/docs/SETUP.md`;
export const MAC_BUILD_URL = `${REPO_URL}#mac-build`;

export type ReleaseState =
  | { status: "loading" }
  | { status: "ready"; info: ReleaseInfo }
  | { status: "error" };

let cached: Promise<ReleaseInfo> | null = null;

function load(): Promise<ReleaseInfo> {
  cached ??= fetch("/api/release")
    .then((r) => (r.ok ? (r.json() as Promise<ReleaseInfo>) : Promise.reject(new Error(String(r.status)))))
    .catch((e) => {
      cached = null; // let a later mount retry
      throw e;
    });
  return cached;
}

/** Latest installers from GitHub Releases (via /api/release, cached at the edge). */
export function useRelease(): ReleaseState {
  const [state, setState] = useState<ReleaseState>({ status: "loading" });
  useEffect(() => {
    let live = true;
    load().then(
      (info) => live && setState({ status: "ready", info }),
      () => live && setState({ status: "error" }),
    );
    return () => {
      live = false;
    };
  }, []);
  return state;
}

export function formatSize(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
}
