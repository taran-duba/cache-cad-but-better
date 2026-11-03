import { motion } from "motion/react";
import { ArrowUpRight, Download as DownloadIcon } from "lucide-react";
import { AppleIcon, WindowsIcon } from "./BrandIcons";
import { hrefFor } from "./DownloadButtons";
import { formatSize, MAC_BUILD_URL, RELEASES_URL, SETUP_URL, useRelease } from "../lib/release";

const ease = [0.16, 1, 0.3, 1] as const;

export function Download() {
  const release = useRelease();
  const info = release.status === "ready" ? release.info : null;
  const macMissing = !!info && !info.mac;

  return (
    <section id="download" className="download" aria-labelledby="download-title">
      <div className="download-aurora" aria-hidden />
      <div className="container">
        <header className="section-head center">
          <p className="eyebrow">Download</p>
          <h2 id="download-title" className="section-title">
            Freeze your CAD in place.
          </h2>
          <p className="section-lede">
            Install it on every PC that opens the robot. One teammate does the{" "}
            <a className="text-link" href={SETUP_URL}>
              one-time team setup
            </a>{" "}
            first, then everyone else just signs in.
          </p>
        </header>

        <div className="dl-grid">
          <motion.article
            className="dl-card featured"
            initial={{ opacity: 0, y: 30 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true, amount: 0.3 }}
            transition={{ duration: 0.6, ease }}
          >
            <div className="dl-os">
              <WindowsIcon size={26} />
              <div>
                <h3>Windows</h3>
                <p>Windows 10 &amp; 11, 64-bit</p>
              </div>
            </div>
            <a className="btn btn-primary btn-lg dl-main" href={hrefFor(release, "windows")}>
              <DownloadIcon size={18} />
              <span className="btn-stack">
                Download installer
                <span className="btn-sub">
                  {info?.windows ? `v${info.windows.version} · .exe · ${formatSize(info.windows.size)}` : ".exe"}
                </span>
              </span>
            </a>
            <a className="dl-alt" href={hrefFor(release, "windowsMsi")}>
              Prefer an MSI for school-managed PCs? <span>Get the .msi</span>
            </a>
            <ul className="dl-notes">
              <li>Reference checking needs SolidWorks installed on the same PC.</li>
              <li>Installs per user. No admin rights needed for the .exe.</li>
            </ul>
          </motion.article>

          <motion.article
            className="dl-card"
            initial={{ opacity: 0, y: 30 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true, amount: 0.3 }}
            transition={{ duration: 0.6, ease, delay: 0.08 }}
          >
            <div className="dl-os">
              <AppleIcon size={26} />
              <div>
                <h3>macOS</h3>
                <p>Apple Silicon (M1 and newer)</p>
              </div>
            </div>
            {macMissing ? (
              <a className="btn btn-lg dl-main" href={MAC_BUILD_URL}>
                <ArrowUpRight size={18} />
                <span className="btn-stack">
                  Build from source
                  <span className="btn-sub">No packaged Mac release yet</span>
                </span>
              </a>
            ) : (
              <a className="btn btn-lg dl-main" href={hrefFor(release, "mac")}>
                <DownloadIcon size={18} />
                <span className="btn-stack">
                  Download for Mac
                  <span className="btn-sub">
                    {info?.mac ? `v${info.mac.version} · .dmg · ${formatSize(info.mac.size)}` : ".dmg"}
                  </span>
                </span>
              </a>
            )}
            <ul className="dl-notes">
              {!macMissing && <li>Not notarized yet: the first time, right-click the app and choose Open.</li>}
              <li>SolidWorks doesn't run on macOS, so reference checking is off. Syncing, locks and check-ins all work.</li>
            </ul>
          </motion.article>
        </div>

        <p className="dl-foot">
          Looking for an older version or release notes?{" "}
          <a className="text-link" href={RELEASES_URL}>
            All releases on GitHub <ArrowUpRight size={14} />
          </a>
        </p>
      </div>
    </section>
  );
}
