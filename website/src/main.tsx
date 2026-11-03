import { StrictMode } from "react";
import { createRoot, hydrateRoot } from "react-dom/client";
import "@fontsource-variable/libre-franklin";
import "@fontsource-variable/space-grotesk";
import "@fontsource-variable/jetbrains-mono";
import "./styles/base.css";
import "./styles/site.css";
import "./styles/story.css";
import "./styles/sections.css";
import "./styles/legal.css";
import "./styles/mock.css";
import "./styles/mock-scenes.css";
import { routeFor } from "./routes";

const { Page } = routeFor(window.location.pathname);
const root = document.getElementById("root")!;
const tree = (
  <StrictMode>
    <Page />
  </StrictMode>
);

// Built pages arrive prerendered, so hydrate them; `pnpm dev` serves an empty shell.
if (root.hasChildNodes()) hydrateRoot(root, tree);
else createRoot(root).render(tree);
