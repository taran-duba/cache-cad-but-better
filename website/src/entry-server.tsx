import { StrictMode } from "react";
import { renderToString } from "react-dom/server";
import { ROUTES, routeFor } from "./routes";

export { ROUTES };

/** Render one page to HTML at build time (see scripts/prerender.mjs). */
export function render(path: string): string {
  const { Page } = routeFor(path);
  return renderToString(
    <StrictMode>
      <Page />
    </StrictMode>,
  );
}
