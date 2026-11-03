import { MotionConfig } from "motion/react";
import { Nav } from "./components/Nav";
import { Hero } from "./components/Hero";
import { Story } from "./components/Story";
import { SyncFlow } from "./components/SyncFlow";
import { Features } from "./components/Features";
import { Download } from "./components/Download";
import { Footer } from "./components/Footer";

export default function App() {
  return (
    // "user": transforms are dropped for anyone with reduced motion turned on; fades remain.
    <MotionConfig reducedMotion="user">
      <a className="skip-link" href="#main">
        Skip to content
      </a>
      <div className="backdrop" aria-hidden />
      <Nav />
      <main id="main">
        <span id="top" />
        <Hero />
        <Story />
        <SyncFlow />
        <Features />
        <Download />
      </main>
      <Footer />
    </MotionConfig>
  );
}
