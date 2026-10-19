//! Google OAuth for an installed app: loopback redirect + PKCE.
//!
//! Each team creates its own OAuth client in Google Cloud (see docs/SETUP.md)
//! and puts the client id/secret in the app config. For desktop clients the
//! "secret" is not confidential; PKCE protects the code exchange. The refresh
//! token is stored in the OS credential store (Windows Credential Manager).

use std::time::{Duration, Instant};

use base64::Engine;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::error::{Error, Result};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/drive openid email";
const KEYRING_SERVICE: &str = "cadsync";
const KEYRING_USER: &str = "google-refresh-token";

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
}

impl OAuthClient {
    /// The client compiled into this build (see build.rs), if any.
    pub fn built_in() -> Option<Self> {
        let id = option_env!("CADSYNC_BUILTIN_CLIENT_ID")?;
        (!id.is_empty()).then(|| Self {
            client_id: id.to_string(),
            client_secret: option_env!("CADSYNC_BUILTIN_CLIENT_SECRET").unwrap_or_default().to_string(),
        })
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

struct AccessToken {
    token: String,
    expires_at: Instant,
}

pub struct Auth {
    client: OAuthClient,
    http: reqwest::Client,
    refresh_token: String,
    access: Mutex<Option<AccessToken>>,
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn random_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    b64url(&buf)
}

fn keyring_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| Error::Auth(e.to_string()))
}

impl Auth {
    /// Restore a session from the credential store, if the user signed in before.
    pub fn from_saved(client: OAuthClient) -> Option<Self> {
        let refresh_token = keyring_entry().ok()?.get_password().ok()?;
        Some(Self::new(client, refresh_token))
    }

    fn new(client: OAuthClient, refresh_token: String) -> Self {
        Self { client, http: reqwest::Client::new(), refresh_token, access: Mutex::new(None) }
    }

    pub fn sign_out() -> Result<()> {
        match keyring_entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Auth(e.to_string())),
        }
    }

    /// Run the browser sign-in. `open_browser` is handed the consent URL.
    pub async fn sign_in(client: OAuthClient, open_browser: impl FnOnce(&str)) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let verifier = random_token();
        let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
        let state = random_token();

        let mut url = url::Url::parse(AUTH_URL).expect("static url");
        url.query_pairs_mut()
            .append_pair("client_id", &client.client_id)
            .append_pair("redirect_uri", &redirect)
            .append_pair("response_type", "code")
            .append_pair("scope", SCOPE)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent");
        open_browser(url.as_str());

        let code = tokio::time::timeout(Duration::from_secs(300), wait_for_code(&listener, &state))
            .await
            .map_err(|_| Error::Auth("sign-in timed out".into()))??;

        let http = reqwest::Client::new();
        let resp: TokenResponse = check(
            http.post(TOKEN_URL)
                .form(&[
                    ("client_id", client.client_id.as_str()),
                    ("client_secret", client.client_secret.as_str()),
                    ("code", code.as_str()),
                    ("code_verifier", verifier.as_str()),
                    ("grant_type", "authorization_code"),
                    ("redirect_uri", redirect.as_str()),
                ])
                .send()
                .await?,
        )
        .await?
        .json()
        .await?;

        let refresh = resp
            .refresh_token
            .ok_or_else(|| Error::Auth("Google did not return a refresh token".into()))?;
        keyring_entry()?.set_password(&refresh).map_err(|e| Error::Auth(e.to_string()))?;
        let auth = Self::new(client, refresh);
        *auth.access.lock().await = Some(AccessToken {
            token: resp.access_token,
            expires_at: Instant::now() + Duration::from_secs(resp.expires_in),
        });
        Ok(auth)
    }

    /// Forget the cached access token (after Drive rejects it with 401).
    pub async fn invalidate(&self) {
        *self.access.lock().await = None;
    }

    /// A valid access token, refreshed when it is within a minute of expiring.
    pub async fn access_token(&self) -> Result<String> {
        let mut guard = self.access.lock().await;
        if let Some(t) = guard.as_ref() {
            if t.expires_at > Instant::now() + Duration::from_secs(60) {
                return Ok(t.token.clone());
            }
        }
        let resp: TokenResponse = check(
            self.http
                .post(TOKEN_URL)
                .form(&[
                    ("client_id", self.client.client_id.as_str()),
                    ("client_secret", self.client.client_secret.as_str()),
                    ("refresh_token", self.refresh_token.as_str()),
                    ("grant_type", "refresh_token"),
                ])
                .send()
                .await?,
        )
        .await?
        .json()
        .await?;
        let token = resp.access_token.clone();
        *guard = Some(AccessToken {
            token: resp.access_token,
            expires_at: Instant::now() + Duration::from_secs(resp.expires_in),
        });
        Ok(token)
    }
}

async fn check(resp: reqwest::Response) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    Err(token_error(status, &body))
}

/// Only a 400 `invalid_grant` means the sign-in is really gone (revoked,
/// expired, password change). Rate limits and server errors are temporary and
/// must not sign anyone out.
fn token_error(status: u16, body: &str) -> Error {
    let reason = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error"].as_str().map(String::from))
        .unwrap_or_default();
    match (status, reason.as_str()) {
        (400, "invalid_grant") => Error::SignInExpired,
        (429, _) | (500..=599, _) => Error::Network(format!("Google sign-in service returned {status}")),
        _ => Error::Auth(format!("token endpoint returned {status}: {reason}")),
    }
}

/// Accept connections on the loopback listener until Google redirects back.
async fn wait_for_code(listener: &TcpListener, expected_state: &str) -> Result<String> {
    loop {
        let (mut sock, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await?;
        let req = String::from_utf8_lossy(&buf[..n]);
        let Some(target) = req.lines().next().and_then(|l| l.split_whitespace().nth(1)) else {
            continue;
        };
        let Ok(url) = url::Url::parse(&format!("http://localhost{target}")) else { continue };
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        if q.is_empty() {
            continue; // favicon or similar
        }

        let (outcome, page) = if let Some(err) = q.get("error") {
            (Err(Error::Auth(format!("Google sign-in failed: {err}"))), "Sign-in failed. You can close this tab.")
        } else if q.get("state").map(String::as_str) != Some(expected_state) {
            (Err(Error::Auth("sign-in state mismatch".into())), "Sign-in failed. You can close this tab.")
        } else if let Some(code) = q.get("code") {
            (Ok(code.clone()), "Signed in to cadsync. You can close this tab.")
        } else {
            continue;
        };
        let body = format!("<!doctype html><meta charset=utf-8><p style=\"font-family:sans-serif\">{page}</p>");
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        return outcome;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_invalid_grant_means_signed_out() {
        assert!(matches!(token_error(400, r#"{"error":"invalid_grant"}"#), Error::SignInExpired));
        assert!(matches!(token_error(503, r#"{"error":"invalid_grant"}"#), Error::Network(_)));
        assert!(matches!(token_error(429, ""), Error::Network(_)));
        assert!(matches!(token_error(400, r#"{"error":"invalid_client"}"#), Error::Auth(_)));
        assert!(token_error(503, "").is_transient());
        assert!(!Error::SignInExpired.is_transient());
    }
}
