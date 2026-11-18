//! Spotify OAuth 2.0 Authorization Code with PKCE.
//!
//! Flow:
//!   1. Generate a high-entropy code verifier and its S256 challenge.
//!   2. Start a throwaway loopback HTTP server on 127.0.0.1:<port>.
//!   3. Open the user's default browser at the Spotify authorize URL.
//!   4. Spotify redirects to the loopback URI with `?code=...&state=...`.
//!   5. Exchange the code for tokens at /api/token (no client secret).
//!   6. Store the refresh token in the Windows Credential Manager.
//!
//! The client secret is never used, and the refresh token never leaves Rust.

use crate::config::{self, Settings};
use crate::error::{AppError, AppResult};
use crate::spotify::TokenSet;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

const AUTH_ENDPOINT: &str = "https://accounts.spotify.com/authorize";
const TOKEN_ENDPOINT: &str = "https://accounts.spotify.com/api/token";

/// Result of a successful authorization round-trip.
pub struct AuthOutcome {
    pub tokens: TokenSet,
}

/// A pending login. Held by the app state so it can be cancelled.
pub struct PendingLogin {
    pub cancel: Sender<()>,
}

/// Builds the browser URL the user must visit.
pub fn authorize_url(client_id: &str, redirect_uri: &str, challenge: &str, state: &str) -> String {
    // Parameters are percent-encoded by hand: this keeps the dependency surface
    // small and the URL is short enough that a generic encoder would be overkill.
    let scopes = config::SCOPES.replace(' ', "%20");
    format!(
        "{AUTH_ENDPOINT}?response_type=code\
&client_id={client_id}\
&redirect_uri={redirect_uri}\
&scope={scopes}\
&state={state}\
&code_challenge_method=S256\
&code_challenge={challenge}"
    )
}

/// Generates a PKCE verifier and matching S256 challenge.
pub fn generate_pkce() -> (String, String) {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

/// Generates the anti-CSRF `state` value.
pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Runs the whole interactive login on the calling thread.
///
/// Callers run this on a worker thread; the cancel channel lets the UI abort a
/// sign-in that is waiting on the browser.
pub fn perform_login(settings: &Settings, cancel: &Receiver<()>) -> AppResult<AuthOutcome> {
    run_login(settings, cancel)
}

/// Convenience wrapper that spawns the blocking login on its own thread.
pub fn spawn_login(settings: Settings, cancel: Receiver<()>) -> Receiver<AppResult<AuthOutcome>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let result = run_login(&settings, &cancel);
        let _ = tx.send(result);
    });
    rx
}

fn run_login(settings: &Settings, cancel: &Receiver<()>) -> AppResult<AuthOutcome> {
    let client_id = settings.client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(AppError::Local(
            "Add your Spotify client id in Settings before connecting.".into(),
        ));
    }

    let (verifier, challenge) = generate_pkce();
    let state = generate_state();

    // Bind first: if the port is taken we fail before opening a browser.
    let listener = TcpListener::bind(("127.0.0.1", settings.redirect_port)).map_err(|e| {
        AppError::Local(format!(
            "Port {} is unavailable for the Spotify sign-in redirect ({}). Close whatever is using it and retry.",
            settings.redirect_port, e
        ))
    })?;

    let redirect_uri = config::redirect_uri(settings.redirect_port);
    let url = authorize_url(&client_id, &redirect_uri, &challenge, &state);

    open_in_browser(&url)?;

    let code = wait_for_code(&listener, &state, cancel)?;
    let tokens = exchange_code(&client_id, &redirect_uri, &code, &verifier)?;

    Ok(AuthOutcome { tokens })
}

/// Waits for the browser to hit the loopback redirect URI.
fn wait_for_code(
    listener: &TcpListener,
    expected_state: &str,
    cancel: &Receiver<()>,
) -> AppResult<String> {
    // Spotify can be slow to redirect; 5 minutes is generous but bounded.
    let _ = listener.set_nonblocking(true);

    let deadline = std::time::Instant::now() + Duration::from_secs(300);
    loop {
        if cancel.try_recv().is_ok() {
            return Err(AppError::Local("Sign-in cancelled.".into()));
        }
        if std::time::Instant::now() > deadline {
            return Err(AppError::Local(
                "Sign-in timed out after 5 minutes. Try again.".into(),
            ));
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let outcome = handle_redirect_once(stream, expected_state);
                match outcome {
                    Ok(Some(code)) => return Ok(code),
                    Ok(None) => {
                        // Browser asked for /favicon.ico or similar; keep waiting.
                        continue;
                    }
                    Err(err) => return Err(err),
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => return Err(AppError::Local(format!("redirect listener failed: {e}"))),
        }
    }
}

/// Parses a single HTTP request from the browser.
///
/// Returns `Ok(None)` for requests that are not the OAuth callback.
fn handle_redirect_once(mut stream: TcpStream, expected_state: &str) -> AppResult<Option<String>> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let mut buffer = [0u8; 4096];
    let read = stream
        .read(&mut buffer)
        .map_err(|e| AppError::Local(format!("could not read redirect: {e}")))?;
    if read == 0 {
        return Ok(None);
    }
    let request = String::from_utf8_lossy(&buffer[..read]);

    let Some(request_line) = request.lines().next() else {
        return Ok(None);
    };
    let mut parts = request_line.split_whitespace();
    let _method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");

    if !target.starts_with(config::REDIRECT_PATH) && !target.starts_with("/") {
        return Ok(None);
    }
    if !target.contains("code=") && !target.contains("error=") {
        // Not the callback (e.g. /favicon.ico).
        let body = b"not found";
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(body);
        let _ = stream.flush();
        return Ok(None);
    }

    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let params = parse_query(query);

    // --- CSRF check --------------------------------------------------------
    // Spotify includes state on both success and error redirects. Reject
    // unsolicited callbacks before displaying any callback-provided text.
    match params.get("state") {
        Some(returned) if returned == expected_state => {}
        Some(_) => {
            write_html(
                &mut stream,
                400,
                "Sign-in failed",
                "The response did not match this sign-in attempt. Please try again.",
            )?;
            return Err(AppError::Local(
                "Spotify sign-in response failed the state check. Try again.".into(),
            ));
        }
        None => {
            write_html(
                &mut stream,
                400,
                "Sign-in failed",
                "Spotify did not return a state value. Please try again.",
            )?;
            return Err(AppError::Local(
                "Spotify sign-in response was missing its state value.".into(),
            ));
        }
    }

    // --- Spotify reported an error on the consent screen -------------------
    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| error.clone());
        write_html(&mut stream, 400, "Authorization failed", &description)?;
        return Err(AppError::Local(format!(
            "Spotify authorization failed: {description}"
        )));
    }

    let Some(code) = params.get("code") else {
        write_html(
            &mut stream,
            400,
            "Sign-in failed",
            "Spotify did not return an authorization code.",
        )?;
        return Err(AppError::Local(
            "Spotify sign-in response was missing its authorization code.".into(),
        ));
    };

    write_html(
        &mut stream,
        200,
        "Sonic is connected",
        "You can close this tab and return to Sonic.",
    )?;

    Ok(Some(code.clone()))
}

/// Minimal HTML page shown in the browser after the redirect lands.
fn write_html(stream: &mut TcpStream, status: u16, title: &str, detail: &str) -> AppResult<()> {
    let reason = if status == 200 { "OK" } else { "Bad Request" };
    let title = escape_html(title);
    let detail = escape_html(detail);
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
<style>body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0b0b0f;\
color:#fff;font:15px/1.5 -apple-system,Segoe UI,Roboto,sans-serif}}\
.card{{max-width:420px;padding:32px 36px;border-radius:20px;background:#15151b;\
border:1px solid rgba(255,255,255,.12);box-shadow:0 24px 60px rgba(0,0,0,.5)}}\
h1{{margin:0 0 8px;font-size:19px;font-weight:600}}\
p{{margin:0;color:#a1a1aa}}</style></head>\
<body><div class=\"card\"><h1>{title}</h1><p>{detail}</p></div></body></html>"
    );

    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| AppError::Local(format!("could not answer browser: {e}")))?;
    let _ = stream.flush();
    Ok(())
}

/// Escapes dynamic text for insertion into an HTML text node.
fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::escape_html;

    #[test]
    fn escapes_html_text_and_attribute_delimiters() {
        assert_eq!(
            escape_html("<script>alert(\"x\")</script>&'"),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;&amp;&#39;"
        );
    }
}

/// Exchanges an authorization code for tokens.
fn exchange_code(
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> AppResult<TokenSet> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ];

    // Spotify's token endpoint requires `application/x-www-form-urlencoded` in the
    // *body*. Sending these as query parameters makes Spotify reply with its HTML
    // error page instead of the JSON API, which surfaced as an opaque "HTTP 400"
    // even when the authorization code was perfectly valid.
    let response = ureq::post(TOKEN_ENDPOINT)
        .send_form(form)
        .map_err(AppError::from)?;

    parse_token_response(response)
}

/// Parses a token endpoint response, tolerating a 4xx JSON body.
pub fn parse_token_response(mut response: ureq::http::Response<ureq::Body>) -> AppResult<TokenSet> {
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| AppError::Network(format!("unreadable token response: {e}")))?;

    if status >= 400 {
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("error_description")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        v.get("error")
                            .and_then(|e| e.as_str())
                            .map(|s| s.to_string())
                    })
            })
            .unwrap_or_else(|| format!("HTTP {status}"));

        return Err(if status == 400 && message.contains("invalid_grant") {
            AppError::RefreshFailed(
                "Spotify rejected the authorization code. Start the sign-in again.".into(),
            )
        } else {
            AppError::Local(format!("Spotify token request failed: {message}"))
        });
    }

    let parsed: TokenSet = serde_json::from_str(&body)
        .map_err(|e| AppError::Local(format!("unexpected token response: {e}")))?;

    if parsed.access_token.is_empty() {
        return Err(AppError::Local(
            "Spotify returned an empty access token.".into(),
        ));
    }
    Ok(parsed)
}

/// Percent-decodes a query string into a map.
fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        map.insert(percent_decode(key), percent_decode(value));
    }
    map
}

/// Decodes `application/x-www-form-urlencoded` escapes, including `+` as space.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Opens a URL with the system default handler.
///
/// Uses `rundll32` on Windows, which needs no extra crate and honours the
/// user's browser choice.
fn open_in_browser(url: &str) -> AppResult<()> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    {
        // Do NOT use `cmd /c start "" <url>` here. cmd.exe treats `&` as a command
        // separator, and a Spotify authorize URL is full of them (`&client_id=`,
        // `&redirect_uri=`, `&state=`, ...). cmd splits the URL apart and tries to
        // execute the fragments, so the browser never opens and this returns
        // exit code 1 — which is exactly how the first login attempt failed.
        //
        // `rundll32 url.dll,FileProtocolHandler` hands the whole string to the
        // shell as a single argument, so the query string survives intact.
        let status = Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .and_then(|mut child| child.wait());

        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(AppError::Local(format!(
                "could not open a browser (exit code {:?})",
                status.code()
            ))),
            Err(e) => Err(AppError::Local(format!("could not open a browser: {e}"))),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let program = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let status = Command::new(program)
            .arg(url)
            .spawn()
            .and_then(|mut child| child.wait());

        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(AppError::Local(format!(
                "could not open a browser (exit code {:?})",
                status.code()
            ))),
            Err(e) => Err(AppError::Local(format!("could not open a browser: {e}"))),
        }
    }
}

/// Human-readable description of the redirect URI the user must allow-list.
pub fn redirect_uri_hint(settings: &Settings) -> String {
    config::redirect_uri(settings.redirect_port)
}
