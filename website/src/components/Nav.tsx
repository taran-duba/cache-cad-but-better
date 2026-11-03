import { useState } from "react";
import { motion, useMotionValueEvent, useScroll } from "motion/react";
import { GitHubIcon, Logo } from "./BrandIcons";
import { REPO_URL } from "../lib/release";

/** `home` is false on the legal pages, where section links have to go back to the homepage. */
export function Nav({ home = true }: { home?: boolean }) {
  const base = home ? "" : "/";
  const { scrollY } = useScroll();
  const [scrolled, setScrolled] = useState(false);
  useMotionValueEvent(scrollY, "change", (y) => setScrolled(y > 24));

  return (
    <motion.header
      className={`nav ${scrolled ? "scrolled" : ""}`}
      initial={{ y: -20, opacity: 0 }}
      animate={{ y: 0, opacity: 1 }}
      transition={{ duration: 0.6, ease: [0.16, 1, 0.3, 1] }}
    >
      <div className="container nav-inner">
        <a href={home ? "#top" : "/"} className="nav-brand" aria-label="Permafrost home">
          <Logo size={26} />
          Permafrost
        </a>
        <nav className="nav-links" aria-label="Sections">
          <a href={`${base}#how`}>How it works</a>
          <a href={`${base}#features`}>Features</a>
          <a href={`${base}#download`}>Download</a>
        </nav>
        <div className="nav-actions">
          <a className="btn btn-ghost nav-gh" href={REPO_URL} aria-label="Permafrost on GitHub">
            <GitHubIcon size={18} />
          </a>
          <a className="btn btn-primary" href={`${base}#download`}>
            Download
          </a>
        </div>
      </div>
    </motion.header>
  );
}
