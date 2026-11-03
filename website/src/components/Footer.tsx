import { Logo } from "./BrandIcons";
import { REPO_URL, RELEASES_URL, SETUP_URL } from "../lib/release";

export function Footer() {
  return (
    <footer className="footer">
      <div className="container footer-inner">
        <div className="footer-brand">
          <span className="nav-brand">
            <Logo size={24} />
            Permafrost
          </span>
          <p>SolidWorks file sharing over Google Drive, made for FRC teams.</p>
          <p className="footer-data">
            Permafrost uses your Google account only to sync the CAD files in the shared drive or folder you choose. Your
            files go straight between your computer and Google, never through us.
          </p>
        </div>
        <nav className="footer-links" aria-label="Footer">
          <a href={REPO_URL}>GitHub</a>
          <a href={RELEASES_URL}>Releases</a>
          <a href={SETUP_URL}>Team setup guide</a>
          <a href="/privacy-policy">Privacy Policy</a>
          <a href="/terms-of-service">Terms of Service</a>
        </nav>
      </div>
      <div className="container">
        <p className="footer-legal">
          SOLIDWORKS is a trademark of Dassault Systèmes. Google Drive is a trademark of Google LLC. FIRST and FRC are
          trademarks of FIRST. Permafrost is not affiliated with or endorsed by any of them.
        </p>
      </div>
    </footer>
  );
}
