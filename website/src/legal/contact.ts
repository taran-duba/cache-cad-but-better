import { REPO_URL } from "../lib/release";

/**
 * Where people send privacy questions and requests. Google's OAuth review expects a way to
 * reach you; set `email` to a monitored address to show it on both legal pages.
 */
export const CONTACT: { email: string | null; issues: string } = {
  email: null,
  issues: `${REPO_URL}/issues`,
};

/** Shown at the top of both pages. Update it whenever either page changes. */
export const EFFECTIVE_DATE = "November 2, 2026";
