// Finds the newest Permafrost installers on GitHub Releases.
//   GET /api/release              -> JSON describing the latest Windows and macOS downloads
//   GET /download/windows         -> 302 to the latest Windows setup .exe   (rewritten to ?platform=windows)
//   GET /download/windows-msi     -> 302 to the latest .msi
//   GET /download/mac             -> 302 to the latest .dmg
// Falls back to the Releases page whenever an installer can't be found.

const REPO = process.env.GITHUB_REPO || "taran-duba/permafrost";
const RELEASES_PAGE = `https://github.com/${REPO}/releases`;
const CACHE = "public, max-age=60, s-maxage=600, stale-while-revalidate=86400";

type GhAsset = { name: string; browser_download_url: string; size: number };
type GhRelease = {
  tag_name: string;
  html_url: string;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  assets: GhAsset[];
};

export type Installer = { version: string; url: string; size: number; file: string; publishedAt: string | null };
export type ReleaseInfo = {
  releasesUrl: string;
  windows: Installer | null;
  windowsMsi: Installer | null;
  mac: Installer | null;
};

const MATCHERS: Record<keyof Omit<ReleaseInfo, "releasesUrl">, (name: string) => number> = {
  windows: (n) => (/-setup\.exe$/i.test(n) ? 1 : /\.exe$/i.test(n) ? 0.5 : 0),
  windowsMsi: (n) => (/\.msi$/i.test(n) ? 1 : 0),
  // Prefer the Apple Silicon build; any .dmg will do otherwise.
  mac: (n) => (/\.dmg$/i.test(n) ? (/aarch64|arm64|universal/i.test(n) ? 1 : 0.5) : 0),
};

function find(releases: GhRelease[], score: (name: string) => number): Installer | null {
  for (const r of releases) {
    let best: GhAsset | null = null;
    let bestScore = 0;
    for (const a of r.assets) {
      const s = score(a.name);
      if (s > bestScore) [best, bestScore] = [a, s];
    }
    if (best)
      return {
        version: r.tag_name.replace(/^v/i, ""),
        url: best.browser_download_url,
        size: best.size,
        file: best.name,
        publishedAt: r.published_at,
      };
  }
  return null;
}

async function lookup(): Promise<ReleaseInfo> {
  const headers: Record<string, string> = {
    Accept: "application/vnd.github+json",
    "User-Agent": "permafrost-website",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;

  const res = await fetch(`https://api.github.com/repos/${REPO}/releases?per_page=20`, { headers });
  if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
  const all = (await res.json()) as GhRelease[];
  // Stable releases only, newest first (GitHub already sorts them that way).
  const stable = all.filter((r) => !r.draft && !r.prerelease);
  return {
    releasesUrl: RELEASES_PAGE,
    windows: find(stable, MATCHERS.windows),
    windowsMsi: find(stable, MATCHERS.windowsMsi),
    mac: find(stable, MATCHERS.mac),
  };
}

const PLATFORMS: Record<string, keyof typeof MATCHERS> = {
  windows: "windows",
  win: "windows",
  "windows-msi": "windowsMsi",
  msi: "windowsMsi",
  mac: "mac",
  macos: "mac",
};

export default {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const platform = url.searchParams.get("platform");

    if (platform !== null) {
      const key = PLATFORMS[platform.toLowerCase()];
      let target = RELEASES_PAGE;
      if (key) {
        try {
          target = (await lookup())[key]?.url ?? RELEASES_PAGE;
        } catch {
          // GitHub unreachable or rate limited: the Releases page still works.
        }
      }
      return new Response(null, { status: 302, headers: { Location: target, "Cache-Control": CACHE } });
    }

    try {
      return Response.json(await lookup(), { headers: { "Cache-Control": CACHE } });
    } catch (e) {
      return Response.json(
        { error: e instanceof Error ? e.message : "lookup failed", releasesUrl: RELEASES_PAGE },
        { status: 502, headers: { "Cache-Control": "public, s-maxage=60" } },
      );
    }
  },
};
