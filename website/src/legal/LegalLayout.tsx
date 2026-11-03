import type { ReactNode } from "react";
import { motion } from "motion/react";
import { Nav } from "../components/Nav";
import { Footer } from "../components/Footer";
import { CONTACT, EFFECTIVE_DATE } from "./contact";

export type TocItem = { id: string; label: string };

/** Shared frame for the privacy policy and terms: page header, sticky contents, prose column. */
export function LegalLayout({
  eyebrow,
  title,
  summary,
  toc,
  children,
}: {
  eyebrow: string;
  title: string;
  summary: ReactNode;
  toc: TocItem[];
  children: ReactNode;
}) {
  return (
    <>
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <div className="backdrop" aria-hidden>
        <div className="backdrop-glow a" />
        <div className="backdrop-grain" />
      </div>
      <Nav home={false} />
      <main id="main" className="legal">
        <div className="container legal-head">
          <motion.div
            initial={{ opacity: 0, y: 14 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.6, ease: [0.16, 1, 0.3, 1] }}
          >
            <p className="eyebrow">{eyebrow}</p>
            <h1 className="legal-title">{title}</h1>
            <p className="legal-date">
              Effective <time>{EFFECTIVE_DATE}</time>
            </p>
          </motion.div>
        </div>
        <div className="container legal-grid">
          <nav className="legal-toc" aria-label="On this page">
            <p className="legal-toc-title">On this page</p>
            <ol>
              {toc.map((t) => (
                <li key={t.id}>
                  <a href={`#${t.id}`}>{t.label}</a>
                </li>
              ))}
            </ol>
          </nav>
          <article className="prose">
            <aside className="legal-summary" aria-label="Summary">
              {summary}
            </aside>
            {children}
          </article>
        </div>
      </main>
      <Footer />
    </>
  );
}

export function Section({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section id={id} className="prose-section" aria-labelledby={`${id}-h`}>
      <h2 id={`${id}-h`}>{title}</h2>
      {children}
    </section>
  );
}

/** How to reach the maintainers, used in both documents. */
export function ContactLine() {
  return (
    <p>
      {CONTACT.email && (
        <>
          Email <a href={`mailto:${CONTACT.email}`}>{CONTACT.email}</a>, or open
        </>
      )}
      {!CONTACT.email && "Open"} an issue at <a href={CONTACT.issues}>{CONTACT.issues.replace("https://", "")}</a>.
      {CONTACT.email
        ? " Please don't post personal information or file contents in a public issue."
        : " Issues are public, so please don't include personal information or file contents. Ask for a private way to follow up and we'll reply there."}
    </p>
  );
}
