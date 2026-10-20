//! Google Drive v3 over HTTPS. Every call retries transient failures
//! (rate limits, 5xx, network) with exponential backoff and jitter.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use rand::Rng;
use reqwest::{Method, RequestBuilder, Response};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use super::auth::Auth;
use super::*;
use crate::error::{Error, Result};

const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3";
const FILE_FIELDS: &str = "id,name,parents,mimeType,md5Checksum,size,headRevisionId,createdTime,\
modifiedTime,trashed,appProperties,contentRestrictions,lastModifyingUser(emailAddress)";
/// Resumable upload chunk; must be a multiple of 256 KiB.
const CHUNK: u64 = 8 * 1024 * 1024;
const MAX_ATTEMPTS: u32 = 6;

pub struct HttpDrive {
    http: reqwest::Client,
    auth: Arc<Auth>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiFile {
    id: String,
    name: String,
    #[serde(default)]
    parents: Vec<String>,
    mime_type: String,
    md5_checksum: Option<String>,
    size: Option<String>,
    head_revision_id: Option<String>,
    #[serde(default)]
    created_time: String,
    #[serde(default)]
    modified_time: String,
    #[serde(default)]
    trashed: bool,
    #[serde(default)]
    app_properties: BTreeMap<String, String>,
    #[serde(default)]
    content_restrictions: Vec<ApiRestriction>,
    last_modifying_user: Option<ApiUser>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiRestriction {
    #[serde(default)]
    read_only: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiUser {
    email_address: Option<String>,
}

impl From<ApiFile> for RemoteFile {
    fn from(f: ApiFile) -> Self {
        RemoteFile {
            id: f.id,
            name: f.name,
            parent: f.parents.into_iter().next(),
            mime_type: f.mime_type,
            md5: f.md5_checksum,
            size: f.size.and_then(|s| s.parse().ok()),
            head_revision_id: f.head_revision_id,
            created_time: f.created_time,
            modified_time: f.modified_time,
            trashed: f.trashed,
            read_only: f.content_restrictions.iter().any(|r| r.read_only),
            app_properties: f.app_properties,
            last_modifier: f.last_modifying_user.and_then(|u| u.email_address),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiFileList {
    #[serde(default)]
    files: Vec<ApiFile>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiChange {
    file_id: Option<String>,
    #[serde(default)]
    removed: bool,
    file: Option<ApiFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiChangeList {
    #[serde(default)]
    changes: Vec<ApiChange>,
    next_page_token: Option<String>,
    new_start_page_token: Option<String>,
}

impl HttpDrive {
    pub fn new(auth: Arc<Auth>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .expect("http client");
        Self { http, auth }
    }

    /// Send a request built by `build`, retrying transient failures.
    async fn send(&self, build: impl Fn(&reqwest::Client) -> RequestBuilder) -> Result<Response> {
        let mut attempt = 0;
        let mut refreshed = false;
        loop {
            attempt += 1;
            let token = self.auth.access_token().await?;
            let err = match build(&self.http).bearer_auth(token).send().await {
                Ok(resp) if resp.status().is_success() => return Ok(resp),
                Ok(resp) if resp.status().as_u16() == 401 && !refreshed => {
                    // Access token rejected: get a fresh one and try once more.
                    refreshed = true;
                    self.auth.invalidate().await;
                    continue;
                }
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    Error::Drive { status, message: api_message(&body) }
                }
                Err(e) => Error::from(e),
            };
            if !err.is_transient() || attempt >= MAX_ATTEMPTS {
                return Err(err);
            }
            let backoff = 500u64 * 2u64.pow(attempt - 1);
            let jitter = rand::thread_rng().gen_range(0..500);
            tracing::warn!(%err, attempt, "retrying Drive request");
            tokio::time::sleep(Duration::from_millis(backoff + jitter)).await;
        }
    }

    async fn json<T: serde::de::DeserializeOwned>(
        &self,
        build: impl Fn(&reqwest::Client) -> RequestBuilder,
    ) -> Result<T> {
        Ok(self.send(build).await?.json().await?)
    }

    async fn stream_to(&self, url: String, dest: &Path) -> Result<()> {
        let resp = self.send(|c| c.get(&url)).await?;
        let tmp = dest.with_extension("cadsync-part");
        let mut out = tokio::fs::File::create(&tmp).await?;
        let mut body = resp.bytes_stream();
        while let Some(chunk) = body.next().await {
            out.write_all(&chunk?).await?;
        }
        out.sync_all().await?;
        drop(out);
        tokio::fs::rename(&tmp, dest).await?;
        Ok(())
    }

    /// Resumable upload: start a session, then send fixed-size chunks. If a
    /// chunk fails, ask the server how much it has and continue from there.
    async fn resumable(&self, method: Method, url: String, metadata: Value, src: &Path) -> Result<RemoteFile> {
        let size = tokio::fs::metadata(src).await?.len();
        let session = self
            .send(|c| {
                c.request(method.clone(), &url)
                    .header("X-Upload-Content-Length", size)
                    .json(&metadata)
            })
            .await?;
        let location = session
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Error::Drive { status: 0, message: "no upload session".into() })?
            .to_string();

        let mut file = tokio::fs::File::open(src).await?;
        let mut offset = 0u64;
        let mut failures = 0;
        loop {
            let len = CHUNK.min(size - offset);
            let mut buf = vec![0u8; len as usize];
            file.seek(std::io::SeekFrom::Start(offset)).await?;
            file.read_exact(&mut buf).await?;
            let range = if size == 0 {
                "bytes */0".to_string()
            } else {
                format!("bytes {}-{}/{}", offset, offset + len - 1, size)
            };
            let token = self.auth.access_token().await?;
            let resp = self
                .http
                .put(&location)
                .bearer_auth(token)
                .header("Content-Range", range)
                .body(buf)
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let f: ApiFile = r.json().await?;
                    return Ok(f.into());
                }
                Ok(r) if r.status().as_u16() == 308 => {
                    offset = next_offset(&r).unwrap_or(offset + len);
                    failures = 0;
                }
                Ok(r) if r.status().is_client_error() && r.status().as_u16() != 429 => {
                    let status = r.status().as_u16();
                    let body = r.text().await.unwrap_or_default();
                    return Err(Error::Drive { status, message: api_message(&body) });
                }
                _ => {
                    failures += 1;
                    if failures >= MAX_ATTEMPTS {
                        return Err(Error::Network("upload failed repeatedly".into()));
                    }
                    tokio::time::sleep(Duration::from_millis(1000 * 2u64.pow(failures - 1))).await;
                    offset = self.query_offset(&location, size).await.unwrap_or(offset);
                }
            }
        }
    }

    async fn query_offset(&self, location: &str, size: u64) -> Option<u64> {
        let token = self.auth.access_token().await.ok()?;
        let r = self
            .http
            .put(location)
            .bearer_auth(token)
            .header("Content-Range", format!("bytes */{size}"))
            .send()
            .await
            .ok()?;
        if r.status().as_u16() == 308 {
            Some(next_offset(&r).unwrap_or(0))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct SharedDrive {
    pub id: String,
    pub name: String,
}

impl HttpDrive {
    /// Every shared drive the signed-in user is a member of.
    pub async fn list_shared_drives(&self) -> Result<Vec<SharedDrive>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Page {
            #[serde(default)]
            drives: Vec<SharedDrive>,
            next_page_token: Option<String>,
        }
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let page: Page = self
                .json(|c| {
                    let mut q = vec![("pageSize", "100"), ("fields", "nextPageToken,drives(id,name)")];
                    if let Some(t) = &token {
                        q.push(("pageToken", t.as_str()));
                    }
                    c.get(format!("{API}/drives")).query(&q)
                })
                .await?;
            out.extend(page.drives);
            match page.next_page_token {
                Some(t) => token = Some(t),
                None => break,
            }
        }
        out.sort_by_key(|d| d.name.to_lowercase());
        Ok(out)
    }
}

/// Keep the shared drive as the listing scope only if the user can see the
/// drive. Asking Drive about a drive you're not a member of gives 404 (or 403);
/// any other status is a real problem, so the drive scope stays and the error
/// shows up when listing.
fn scope_after_probe(drive_id: String, status: u16) -> Option<String> {
    if matches!(status, 403 | 404) {
        None
    } else {
        Some(drive_id)
    }
}

/// Escape a value placed inside single quotes in a Drive `q` expression.
fn escape_q(v: &str) -> String {
    v.replace('\\', "\\\\").replace('\'', "\\'")
}

/// A folder in the picker.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct BrowseFolder {
    pub id: String,
    pub name: String,
}

impl HttpDrive {
    async fn folder_query(&self, q: &str, extra: &[(&str, &str)]) -> Result<Vec<BrowseFolder>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Page {
            #[serde(default)]
            files: Vec<BrowseFolder>,
            next_page_token: Option<String>,
        }
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let page: Page = self
                .json(|c| {
                    let mut params = vec![
                        ("supportsAllDrives", "true"),
                        ("includeItemsFromAllDrives", "true"),
                        ("pageSize", "200"),
                        ("q", q),
                        ("orderBy", "name_natural"),
                        ("fields", "nextPageToken,files(id,name)"),
                    ];
                    params.extend_from_slice(extra);
                    if let Some(t) = &token {
                        params.push(("pageToken", t.as_str()));
                    }
                    c.get(format!("{API}/files")).query(&params)
                })
                .await?;
            out.extend(page.files);
            match page.next_page_token {
                Some(t) => token = Some(t),
                None => break,
            }
        }
        Ok(out)
    }

    /// Folders directly inside `parent` (any folder id, `root` for My Drive).
    pub async fn child_folders(&self, parent: &str) -> Result<Vec<BrowseFolder>> {
        let q = format!(
            "'{}' in parents and mimeType = '{FOLDER_MIME}' and trashed = false",
            escape_q(parent)
        );
        self.folder_query(&q, &[("corpora", "allDrives")]).await
    }

    /// Folders other people shared with you.
    pub async fn shared_with_me_folders(&self) -> Result<Vec<BrowseFolder>> {
        let q = format!("sharedWithMe = true and mimeType = '{FOLDER_MIME}' and trashed = false");
        self.folder_query(&q, &[("corpora", "user")]).await
    }
}

/// From a 308 response's `Range: bytes=0-N` header, the next byte to send.
fn next_offset(r: &Response) -> Option<u64> {
    let range = r.headers().get("range")?.to_str().ok()?;
    let end: u64 = range.rsplit('-').next()?.parse().ok()?;
    Some(end + 1)
}

fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            let e = &v["error"];
            let reason = e["errors"][0]["reason"].as_str().unwrap_or("");
            e["message"].as_str().map(|m| format!("{m} ({reason})"))
        })
        .unwrap_or_else(|| body.chars().take(300).collect())
}

fn props_json(map: &BTreeMap<String, Option<String>>) -> Value {
    Value::Object(map.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
}

impl DriveApi for HttpDrive {
    async fn whoami(&self) -> Result<String> {
        let v: Value = self.json(|c| c.get(format!("{API}/about")).query(&[("fields", "user(emailAddress)")])).await?;
        Ok(v["user"]["emailAddress"].as_str().unwrap_or_default().to_string())
    }

    async fn folder_info(&self, id: &str) -> Result<FolderInfo> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct F {
            id: String,
            name: String,
            mime_type: String,
            drive_id: Option<String>,
        }
        let f: F = self
            .json(|c| {
                c.get(format!("{API}/files/{id}"))
                    .query(&[("supportsAllDrives", "true"), ("fields", "id,name,mimeType,driveId")])
            })
            .await?;
        // A folder can be in a shared drive without you being a member (someone
        // shared just the folder). Listing "as the drive" is refused then, so
        // only scope to the drive if we can see it.
        let drive_id = match f.drive_id {
            Some(d) => match self.send(|c| c.get(format!("{API}/drives/{d}")).query(&[("fields", "id")])).await {
                Ok(_) => Some(d),
                Err(Error::Drive { status, .. }) => scope_after_probe(d, status),
                Err(e) => return Err(e), // couldn't tell (network); don't guess
            },
            None => None,
        };
        Ok(FolderInfo { is_folder: f.mime_type == FOLDER_MIME, id: f.id, name: f.name, drive_id })
    }

    async fn list_children(&self, scope: Option<&str>, parents: &[String], page_token: Option<&str>) -> Result<FilePage> {
        if parents.is_empty() {
            return Ok(FilePage { files: vec![], next_page_token: None });
        }
        let fields = format!("nextPageToken,files({FILE_FIELDS})");
        let any_parent = parents.iter().map(|p| format!("'{}' in parents", escape_q(p))).collect::<Vec<_>>().join(" or ");
        let q = format!("trashed = false and ({any_parent})");
        let list: ApiFileList = self
            .json(|c| {
                let mut params = vec![
                    ("includeItemsFromAllDrives", "true"),
                    ("supportsAllDrives", "true"),
                    ("pageSize", "1000"),
                    ("q", q.as_str()),
                    ("fields", fields.as_str()),
                ];
                match scope {
                    Some(d) => {
                        params.push(("corpora", "drive"));
                        params.push(("driveId", d));
                    }
                    None => params.push(("corpora", "allDrives")),
                }
                if let Some(t) = page_token {
                    params.push(("pageToken", t));
                }
                c.get(format!("{API}/files")).query(&params)
            })
            .await?;
        Ok(FilePage { files: list.files.into_iter().map(Into::into).collect(), next_page_token: list.next_page_token })
    }

    async fn start_page_token(&self, scope: Option<&str>) -> Result<String> {
        let v: Value = self
            .json(|c| {
                let mut q = vec![("supportsAllDrives", "true")];
                if let Some(d) = scope {
                    q.push(("driveId", d));
                }
                c.get(format!("{API}/changes/startPageToken")).query(&q)
            })
            .await?;
        v["startPageToken"].as_str().map(String::from).ok_or_else(|| Error::Drive {
            status: 0,
            message: "missing startPageToken".into(),
        })
    }

    async fn list_changes(&self, scope: Option<&str>, page_token: &str) -> Result<ChangePage> {
        let fields = format!("nextPageToken,newStartPageToken,changes(fileId,removed,file({FILE_FIELDS}))");
        let list: ApiChangeList = self
            .json(|c| {
                let mut q = vec![
                    ("pageToken", page_token),
                    ("includeItemsFromAllDrives", "true"),
                    ("supportsAllDrives", "true"),
                    ("pageSize", "1000"),
                    ("fields", fields.as_str()),
                ];
                if let Some(d) = scope {
                    q.push(("driveId", d));
                }
                c.get(format!("{API}/changes")).query(&q)
            })
            .await?;
        Ok(ChangePage {
            changes: list
                .changes
                .into_iter()
                .filter_map(|c| {
                    let file: Option<RemoteFile> = c.file.map(Into::into);
                    let file_id = c.file_id.or_else(|| file.as_ref().map(|f| f.id.clone()))?;
                    let removed = c.removed || file.as_ref().is_some_and(|f| f.trashed);
                    Some(Change { file_id, removed, file })
                })
                .collect(),
            next_page_token: list.next_page_token,
            new_start_page_token: list.new_start_page_token,
        })
    }

    async fn get_file(&self, file_id: &str) -> Result<RemoteFile> {
        let f: ApiFile = self
            .json(|c| {
                c.get(format!("{API}/files/{file_id}"))
                    .query(&[("supportsAllDrives", "true"), ("fields", FILE_FIELDS)])
            })
            .await?;
        Ok(f.into())
    }

    async fn download_file(&self, file_id: &str, dest: &Path) -> Result<()> {
        self.stream_to(format!("{API}/files/{file_id}?alt=media&supportsAllDrives=true"), dest).await
    }

    async fn download_revision(&self, file_id: &str, revision_id: &str, dest: &Path) -> Result<()> {
        self.stream_to(format!("{API}/files/{file_id}/revisions/{revision_id}?alt=media"), dest).await
    }

    async fn upload_content(&self, file_id: &str, src: &Path) -> Result<RemoteFile> {
        let url = format!("{UPLOAD}/files/{file_id}?uploadType=resumable&supportsAllDrives=true&fields={FILE_FIELDS}");
        self.resumable(Method::PATCH, url, json!({}), src).await
    }

    async fn create_file(
        &self,
        parent_id: &str,
        name: &str,
        src: &Path,
        app_properties: BTreeMap<String, String>,
    ) -> Result<RemoteFile> {
        let url = format!("{UPLOAD}/files?uploadType=resumable&supportsAllDrives=true&fields={FILE_FIELDS}");
        let meta = json!({ "name": name, "parents": [parent_id], "appProperties": app_properties });
        self.resumable(Method::POST, url, meta, src).await
    }

    async fn create_folder(&self, parent_id: &str, name: &str) -> Result<RemoteFile> {
        let body = json!({ "name": name, "parents": [parent_id], "mimeType": FOLDER_MIME });
        let f: ApiFile = self
            .json(|c| {
                c.post(format!("{API}/files"))
                    .query(&[("supportsAllDrives", "true"), ("fields", FILE_FIELDS)])
                    .json(&body)
            })
            .await?;
        Ok(f.into())
    }

    async fn update_metadata(&self, file_id: &str, patch: MetadataPatch) -> Result<RemoteFile> {
        let mut body = serde_json::Map::new();
        if !patch.app_properties.is_empty() {
            body.insert("appProperties".into(), props_json(&patch.app_properties));
        }
        if let Some((on, reason)) = &patch.read_only {
            let mut r = json!({ "readOnly": on });
            if let (true, Some(reason)) = (on, reason) {
                r["reason"] = json!(reason);
            }
            body.insert("contentRestrictions".into(), json!([r]));
        }
        let body = Value::Object(body);
        let f: ApiFile = self
            .json(|c| {
                c.patch(format!("{API}/files/{file_id}"))
                    .query(&[("supportsAllDrives", "true"), ("fields", FILE_FIELDS)])
                    .json(&body)
            })
            .await?;
        Ok(f.into())
    }

    async fn keep_revision_forever(&self, file_id: &str, revision_id: &str) -> Result<()> {
        self.send(|c| {
            c.patch(format!("{API}/files/{file_id}/revisions/{revision_id}"))
                .json(&json!({ "keepForever": true }))
        })
        .await?;
        Ok(())
    }

    async fn trash_file(&self, file_id: &str) -> Result<()> {
        self.send(|c| {
            c.patch(format!("{API}/files/{file_id}"))
                .query(&[("supportsAllDrives", "true"), ("fields", "id,trashed")])
                .json(&json!({ "trashed": true }))
        })
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_to_the_drive_only_for_members() {
        assert_eq!(scope_after_probe("D".into(), 404), None, "folder shared from a drive you're not in");
        assert_eq!(scope_after_probe("D".into(), 403), None);
        assert_eq!(scope_after_probe("D".into(), 500), Some("D".into()), "real errors aren't treated as 'not a member'");
    }
}
