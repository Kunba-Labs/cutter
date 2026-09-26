//! YouTube Data API v3: an installed-app OAuth flow through a one-shot loopback
//! page, then resumable Shorts uploads scheduled with `publishAt`.
//! ponytail: the refresh token lives in settings (SQLite, user-only file);
//! move it to the keychain if the library ever leaves this Mac.

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use crate::model::YoutubeAuth;

const SCOPES: &str = "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(120)).redirects(0).build()
}

/// Open the consent page, catch the code on 127.0.0.1, trade it for tokens.
pub fn connect(auth: &YoutubeAuth) -> Result<YoutubeAuth, String> {
    if auth.client_id.is_empty() || auth.client_secret.is_empty() {
        return Err("Add the OAuth client id and secret first (Google Cloud console → Credentials → Desktop app).".into());
    }
    let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = match server.server_addr() {
        tiny_http::ListenAddr::IP(a) => a.port(),
        _ => return Err("no port".into()),
    };
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        urlenc(&auth.client_id),
        urlenc(&redirect),
        urlenc(SCOPES)
    );
    let _ = std::process::Command::new("open").arg(&url).spawn();
    let code = loop {
        let req = server.recv_timeout(Duration::from_secs(300)).map_err(|e| e.to_string())?.ok_or("Timed out waiting for Google's redirect")?;
        let u = req.url().to_string();
        if let Some(q) = u.strip_prefix("/callback?") {
            let code = q.split('&').find_map(|kv| kv.strip_prefix("code=")).map(urldec);
            let _ = req.respond(tiny_http::Response::from_string("<html><body style='font-family:system-ui;padding:40px'>Cuttar is connected to YouTube. You can close this tab.</body></html>").with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap()));
            match code {
                Some(c) => break c,
                None => return Err(format!("Google returned no code: {u}")),
            }
        }
        let _ = req.respond(tiny_http::Response::from_string("waiting").with_status_code(404));
    };
    let tok: Value = agent()
        .post("https://oauth2.googleapis.com/token")
        .send_form(&[("code", &code), ("client_id", &auth.client_id), ("client_secret", &auth.client_secret), ("redirect_uri", &redirect), ("grant_type", "authorization_code")])
        .map_err(|e| format!("token: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let refresh = tok["refresh_token"].as_str().ok_or_else(|| format!("no refresh token in {tok}"))?.to_string();
    let access = tok["access_token"].as_str().unwrap_or("").to_string();
    let mut out = auth.clone();
    out.refresh_token = refresh;
    out.channel_title = channel_title(&access).unwrap_or_default();
    Ok(out)
}

fn channel_title(access: &str) -> Result<String, String> {
    let v: Value = agent().get("https://www.googleapis.com/youtube/v3/channels?part=snippet&mine=true").set("Authorization", &format!("Bearer {access}")).call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
    Ok(v["items"][0]["snippet"]["title"].as_str().unwrap_or("").to_string())
}

pub fn access_token(auth: &YoutubeAuth) -> Result<String, String> {
    if auth.refresh_token.is_empty() {
        return Err("YouTube is not connected (Settings › Channels).".into());
    }
    let v: Value = agent()
        .post("https://oauth2.googleapis.com/token")
        .send_form(&[("refresh_token", &auth.refresh_token), ("client_id", &auth.client_id), ("client_secret", &auth.client_secret), ("grant_type", "refresh_token")])
        .map_err(|e| format!("refresh: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    v["access_token"].as_str().map(String::from).ok_or_else(|| format!("refresh failed: {v}"))
}

pub struct Upload<'a> {
    pub path: &'a Path,
    pub title: &'a str,
    pub description: &'a str,
    pub tags: &'a [String],
    pub category_id: &'a str,
    pub privacy: &'a str,
    /// RFC 3339; when set the video is uploaded private and YouTube publishes it then.
    pub publish_at: Option<&'a str>,
}

/// Resumable upload in 8 MB chunks; returns the video id.
pub fn upload(auth: &YoutubeAuth, u: &Upload, mut on_progress: impl FnMut(f64, &str)) -> Result<String, String> {
    let access = access_token(auth)?;
    let bytes = std::fs::read(u.path).map_err(|e| e.to_string())?;
    let total = bytes.len();
    let mut status = json!({ "privacyStatus": u.privacy, "selfDeclaredMadeForKids": false });
    if let Some(p) = u.publish_at {
        status["privacyStatus"] = json!("private");
        status["publishAt"] = json!(p);
    }
    let meta = json!({ "snippet": { "title": u.title, "description": u.description, "tags": u.tags, "categoryId": u.category_id }, "status": status });
    let resp = agent()
        .post("https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status")
        .set("Authorization", &format!("Bearer {access}"))
        .set("X-Upload-Content-Type", "video/mp4")
        .set("X-Upload-Content-Length", &total.to_string())
        .send_json(meta)
        .map_err(|e| format!("start upload: {e}"))?;
    let location = resp.header("Location").ok_or("no upload location")?.to_string();
    const CHUNK: usize = 8 * 1024 * 1024;
    let mut at = 0;
    loop {
        let end = (at + CHUNK).min(total);
        let r = agent()
            .put(&location)
            .set("Authorization", &format!("Bearer {access}"))
            .set("Content-Type", "video/mp4")
            .set("Content-Range", &format!("bytes {}-{}/{}", at, end - 1, total))
            .send_bytes(&bytes[at..end]);
        match r {
            Ok(resp) if resp.status() == 200 || resp.status() == 201 => {
                let v: Value = resp.into_json().map_err(|e| e.to_string())?;
                on_progress(1.0, "uploaded");
                return v["id"].as_str().map(String::from).ok_or_else(|| format!("no id in {v}"));
            }
            Ok(resp) if resp.status() == 308 => {
                at = resp.header("Range").and_then(|r| r.rsplit('-').next()).and_then(|n| n.parse::<usize>().ok()).map(|n| n + 1).unwrap_or(end);
                on_progress(at as f64 / total as f64, &format!("{} of {} MB", at / 1_048_576, total / 1_048_576));
            }
            Ok(resp) => return Err(format!("upload status {}", resp.status())),
            Err(ureq::Error::Status(308, resp)) => {
                at = resp.header("Range").and_then(|r| r.rsplit('-').next()).and_then(|n| n.parse::<usize>().ok()).map(|n| n + 1).unwrap_or(end);
                on_progress(at as f64 / total as f64, &format!("{} of {} MB", at / 1_048_576, total / 1_048_576));
            }
            Err(ureq::Error::Status(code, resp)) => return Err(format!("upload {code}: {}", resp.into_string().unwrap_or_default().chars().take(300).collect::<String>())),
            Err(e) => return Err(format!("upload: {e}")),
        }
        if at >= total {
            return Err("upload ended without a video id".into());
        }
    }
}

fn urlenc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn urldec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 1 && i + 2 <= b.len() - 1 + 1 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..(i + 3).min(s.len())], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url_codec() {
        assert_eq!(urlenc("a b/c"), "a%20b%2Fc");
        assert_eq!(urldec("4%2F0AX%2Bz+q"), "4/0AX+z q");
    }
}
