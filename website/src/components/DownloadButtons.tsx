import { useSyncExternalStore, type ReactNode } from "react";
import { detectPlatform } from "../lib/platform";
import { MAC_BUILD_URL, RELEASES_URL, useRelease, type ReleaseState } from "../lib/release";
import { AppleIcon, WindowsIcon } from "./BrandIcons";

type Choice = { id: "windows" | "mac"; label: string; href: string; icon: ReactNode; available: boolean };

export function hrefFor(state: ReleaseState, id: "windows" | "mac" | "windowsMsi"): string {
  if (state.status === "error") return RELEASES_URL;
  if (state.status === "loading") return `/download/${id === "windowsMsi" ? "windows-msi" : id}`;
  return state.info[id]?.url ?? RELEASES_URL;
}

const noSubscribe = () => () => {};

/** Hero call to action: the visitor's own OS first, the other one beside it. */
export function DownloadButtons() {
  const release = useRelease();
  // "other" while prerendering, so the static HTML and the first client render agree.
  const platform = useSyncExternalStore(noSubscribe, detectPlatform, () => "other" as const);
  const macMissing = release.status === "ready" && !release.info.mac;
  const version =
    release.status === "ready" ? (release.info.windows?.version ?? release.info.mac?.version) : undefined;

  const win: Choice = {
    id: "windows",
    label: "Download for Windows",
    href: hrefFor(release, "windows"),
    icon: <WindowsIcon size={17} />,
    available: true,
  };
  const mac: Choice = {
    id: "mac",
    label: "Download for macOS",
    href: hrefFor(release, "mac"),
    icon: <AppleIcon size={18} />,
    available: !macMissing,
  };
  const [first, second] = platform === "mac" && mac.available ? [mac, win] : [win, mac];

  return (
    <div className="cta">
      <div className="cta-row">
        <a className="btn btn-primary btn-lg" href={first.href}>
          {first.icon}
          {first.label}
        </a>
        {second.available ? (
          <a className="btn btn-lg" href={second.href}>
            {second.icon}
            {second.id === "mac" ? "macOS" : "Windows"}
          </a>
        ) : (
          <a className="btn btn-lg btn-ghost" href={MAC_BUILD_URL}>
            <AppleIcon size={18} />
            macOS: build from source
          </a>
        )}
      </div>
      <p className="cta-meta">
        {version ? <span className="mono">v{version}</span> : <span className="cta-meta-skel" />}
        <span aria-hidden>·</span>
        <span>Windows 10 &amp; 11 (x64){macMissing ? "" : " and Apple Silicon Macs"}</span>
        <span aria-hidden>·</span>
        <span>Source on GitHub</span>
      </p>
    </div>
  );
}
