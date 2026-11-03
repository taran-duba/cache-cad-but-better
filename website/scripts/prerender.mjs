// After `vite build` and the SSR build, write a fully rendered HTML file for every route,
// so each page (including the privacy policy and terms) reads fine without JavaScript.
import { readFile, writeFile, mkdir, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const dist = resolve("dist");
const server = resolve("dist-server");
const { render, ROUTES } = await import(pathToFileURL(join(server, "entry-server.js")).href);
const template = await readFile(join(dist, "index.html"), "utf8");

const escape = (s) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");

for (const route of ROUTES) {
  const html = template
    .replace('<div id="root"></div>', `<div id="root">${render(route.path)}</div>`)
    .replace(/<title>[\s\S]*?<\/title>/, `<title>${escape(route.title)}</title>`)
    .replace(/(<meta\s+name="description"\s+content=")[^"]*(")/, `$1${escape(route.description)}$2`)
    .replace(/(<meta\s+property="og:title"\s+content=")[^"]*(")/, `$1${escape(route.title)}$2`)
    .replace(/(<meta\s+property="og:description"\s+content=")[^"]*(")/, `$1${escape(route.description)}$2`);

  // /privacy-policy -> dist/privacy-policy.html; vercel.json's cleanUrls serves it without ".html".
  const file = route.path === "/" ? join(dist, "index.html") : join(dist, `${route.path.slice(1)}.html`);
  await mkdir(join(file, ".."), { recursive: true });
  await writeFile(file, html);
  console.log(`prerendered ${route.path}`);
}

await rm(server, { recursive: true, force: true });
