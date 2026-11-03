import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import release from "./api/release.ts";

// In `pnpm dev`, serve /api/release and /download/* with the same handler Vercel runs,
// so download buttons behave locally exactly as they do when deployed.
function releaseApi(): Plugin {
  return {
    name: "permafrost-release-api",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const url = new URL(req.url ?? "/", "http://localhost");
        const dl = url.pathname.match(/^\/download\/([\w-]+)\/?$/);
        if (url.pathname !== "/api/release" && !dl) return next();
        if (dl) url.searchParams.set("platform", dl[1]);
        const out = await release.fetch(new Request(url));
        res.statusCode = out.status;
        out.headers.forEach((v, k) => res.setHeader(k, v));
        res.end(Buffer.from(await out.arrayBuffer()));
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), releaseApi()],
});
