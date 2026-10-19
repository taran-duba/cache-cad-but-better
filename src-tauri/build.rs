//! Bakes the team's Google OAuth client into the app, so nobody has to type
//! it in. Source, in order:
//!  1. `CADSYNC_GOOGLE_CLIENT_ID` / `CADSYNC_GOOGLE_CLIENT_SECRET` env vars (CI secrets)
//!  2. `src-tauri/google-oauth.json` — the "Desktop app" client JSON downloaded
//!     from Google Cloud Console, as-is. It's gitignored; never commit it.
//!
//! Without either, the app builds fine but only demo mode can sign in.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=CADSYNC_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=CADSYNC_GOOGLE_CLIENT_SECRET");
    println!("cargo:rerun-if-changed=google-oauth.json");

    let from_env = std::env::var("CADSYNC_GOOGLE_CLIENT_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|id| (id, std::env::var("CADSYNC_GOOGLE_CLIENT_SECRET").unwrap_or_default()));
    let client = from_env.or_else(|| read_client_file(Path::new("google-oauth.json")));

    if let Some((id, secret)) = client {
        println!("cargo:rustc-env=CADSYNC_BUILTIN_CLIENT_ID={id}");
        println!("cargo:rustc-env=CADSYNC_BUILTIN_CLIENT_SECRET={secret}");
    } else {
        println!("cargo:warning=No Google OAuth client found (src-tauri/google-oauth.json or CADSYNC_GOOGLE_CLIENT_ID); sign-in will be unavailable in this build. See docs/SETUP.md.");
    }

    tauri_build::build()
}

/// Google's download looks like `{"installed": {"client_id": "...", "client_secret": "...", ...}}`.
fn read_client_file(path: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
    let c = json.get("installed").or_else(|| json.get("web")).unwrap_or(&json);
    let id = c.get("client_id")?.as_str()?.to_string();
    let secret = c.get("client_secret").and_then(|s| s.as_str()).unwrap_or_default().to_string();
    Some((id, secret))
}
