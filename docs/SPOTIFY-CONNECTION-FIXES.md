# Spotify connection — bugs found and fixed during real use

Both bugs were found by actually connecting a real account, not by inspection.

---

## Bug 1: browser never opened

### Symptom

`start_login` failed immediately:

```
error: "could not open a browser (exit code Some(1))"
```

### Cause

`auth.rs` opened the authorize URL with:

```rust
Command::new("cmd").args(["/c", "start", "", url])
```

cmd.exe treats `&` as a command separator. A Spotify authorize URL is nothing but
`&`-separated parameters:

```
...?response_type=code&client_id=...&redirect_uri=...&scope=...&state=...&code_challenge_method=S256&code_challenge=...
```

So cmd split the URL and tried to execute the fragments:

```
'client_id' is not recognized as an internal or external command
'redirect_uri' is not recognized as an internal or external command
'code_challenge' is not recognized as an internal or external command
```

The browser never opened, and the command exited 1.

### Fix

`rundll32 url.dll,FileProtocolHandler` passes the whole string as a single
argument, so the query string survives intact.

```rust
Command::new("rundll32")
    .args(["url.dll,FileProtocolHandler", url])
```

Verified: exit 0, and Edge actually launched.

### Lesson

Never pass a URL containing `&` through `cmd /c start`. Use the shell API
(`rundll32 url.dll,FileProtocolHandler`) or a crate.

---

## Bug 2: token exchange returned "HTTP 400"

### Symptom

After a successful browser login, the app showed:

```
error: "Could not reach Spotify. Check your connection. (Spotify returned HTTP 400)"
```

The callback page rendered "Sonic is connected" correctly, so the code arrived —
but the exchange failed.

### Cause

`exchange_code` sent the token parameters as **query string**:

```rust
let mut request = ureq::post(TOKEN_ENDPOINT);
for (key, value) in form {
    request = request.query(key, value);   // <-- wrong
}
let response = request.send_empty()...;
```

Spotify's `/api/token` requires `application/x-www-form-urlencoded` in the
**body**. With query parameters it doesn't even reach the token handler — it
returns its generic HTML error page:

```
Content-Type: text/html
```

The app then failed to parse that as JSON and collapsed the real problem into an
opaque `HTTP 400`.

### Proof

Same request, same fake code, two different encodings:

| Encoding | Response |
| --- | --- |
| Query params (the bug) | `text/html` — Spotify's error page |
| Form body (the fix) | `application/json` — `{"error":"invalid_grant","error_description":"Invalid authorization code"}` |

The form-encoded version reaches the real API. That is the difference between
"Spotify rejected my code" and "I have no idea what Spotify is saying".

### Fix

Both call sites now use `send_form`:

`auth.rs` — authorization code exchange:

```rust
let response = ureq::post(TOKEN_ENDPOINT)
    .send_form(form)
    .map_err(AppError::from)?;
```

`spotify.rs` — refresh token grant:

```rust
let response = self.agent.post(TOKEN_ENDPOINT)
    .send_form(form)
    .map_err(AppError::from)?;
```

The refresh path had the identical bug, so the first token refresh after 60
minutes would have failed too. Fixed pre-emptively rather than waiting for it.

### Lesson

Check the `Content-Type` of an error response before trusting its status code.
An HTML body where JSON was expected means the request never reached the handler,
which is a different bug from "the handler rejected my data".

---

## Result after both fixes

```
connected: true
account:   Faithzz · Premium
device:    Web Player (Chrome) — active, volume 100%
track:     Pyramids — Frank Ocean
album:     channel ORANGE
progress:  1:26 / 9:52
```

Playback control verified against the live account:

```
playback_command pause -> ok
playback_command play  -> ok
```

The UI renders the real artwork, title, artist, progress and transport controls,
with no rectangular panel behind the pill.