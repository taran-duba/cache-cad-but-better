export type Platform = "windows" | "mac" | "other";

/** Best guess at the visitor's desktop OS, used only to order the download buttons. */
export function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return "other";
  const nav = navigator as Navigator & { userAgentData?: { platform?: string; mobile?: boolean } };
  if (nav.userAgentData?.mobile) return "other";
  const hint = (nav.userAgentData?.platform || nav.platform || nav.userAgent || "").toLowerCase();
  if (/iphone|ipad|ipod|android/.test(nav.userAgent.toLowerCase())) return "other";
  if (hint.includes("win")) return "windows";
  if (hint.includes("mac")) return "mac";
  return "other";
}
