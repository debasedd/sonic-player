# Sonic — Spotify Client ID setup

Sonic needs **one thing** from you before it can control playback: a Spotify
**Client ID**. This file walks through getting one, and there is no client secret
involved.

---

## 1. Create a Spotify developer app

1. Go to **https://developer.spotify.com/dashboard**
2. Sign in with the Spotify account you want Sonic to control.
   (Any account works — Premium is only needed for volume and seeking on some
   devices.)
3. Click **Create app**.
4. Fill in:
   - **App name**: `Sonic`
   - **App description**: `A floating mini player that controls my own playback`
   - **Redirect URI**: paste exactly the URI Sonic shows you in
     *Settings → Spotify → Redirect URI*. By default that is:

     ```
     http://127.0.0.1:8976/callback
     ```

     Copy it from the app rather than typing it, so there is no typo.
   - **Which API/SDKs will you use?**: tick **Web API**
5. Accept the terms and create the app.
6. On the app's overview page click **Settings** (or **View app settings**) and
   copy the **Client ID**. It is a 32-character hex string.

## 2. Paste it into Sonic

1. Launch Sonic. It opens as a small dark pill near the top of your screen.
2. Click the **gear icon** on the right of the pill.
3. Under **Spotify**, paste the Client ID into the **Client ID** field and press
   **Save**.
4. Click **Connect Spotify**.
5. Your browser opens a Spotify consent screen. Approve it.
6. The browser shows "Sonic is connected — you can close this tab". Close it and
   go back to Sonic. The pill now shows your current track.

That is the whole setup. The Client ID is stored in Sonic's own settings file,
not in the repo.

---

## 3. If the port is already taken

Sonic listens on `127.0.0.1:8976` only for the few seconds while you sign in. If
something else already owns that port, the sign-in fails with a message naming
the port.

Two options:

- Close whatever is using port 8976 and try again, **or**
- Change the port. In the Spotify dashboard add a second redirect URI with your
  chosen port (for example `http://127.0.0.1:9124/callback`), then run Sonic with
  the port overridden before launch:

  ```powershell
  $env:SONIC_SPOTIFY_REDIRECT_PORT = "9124"
  .\Sonic.exe
  ```

  The redirect URI Sonic displays in Settings updates to match.

Spotify requires the redirect host to be a **loopback IP literal** — `127.0.0.1`
works, `localhost` does not.

---

## 4. Scopes Sonic requests

Sonic asks for exactly what it needs:

| Scope | Why |
| --- | --- |
| `user-read-playback-state` | Read what is playing, on which device, shuffle/repeat state |
| `user-modify-playback-state` | Play, pause, skip, seek, volume, shuffle, repeat |
| `user-read-currently-playing` | Cheap one-second polling for track and progress |
| `user-read-private` | Read your display name and product (Premium/Free) |
| `user-read-email` | Account label in Settings |

Spotify shows these to you on the consent screen. Sonic never asks for your
password and never sees it.

---

## 5. Environment variable alternative

If you are running from source and would rather not paste the ID in the UI every
time, Sonic reads it from the environment:

```powershell
$env:SONIC_SPOTIFY_CLIENT_ID = "your32characterclientid"
$env:SONIC_SPOTIFY_REDIRECT_PORT = "8976"   # optional
npm run tauri dev
```

The environment value overrides the saved setting. See `.env.example`.

---

## 6. Where things are stored

| What | Where |
| --- | --- |
| Client ID, appearance, behaviour | `%APPDATA%\com.sonic.player\settings.json` |
| Window position | `%APPDATA%\com.sonic.player\geometry.json` |
| Refresh token | **Windows Credential Manager**, entry `Sonic` (DPAPI-encrypted, per-user). Falls back to a DPAPI-encrypted file only if Credential Manager refuses the write. |

You can revoke Sonic's access at any time from
**https://www.spotify.com/account/apps/** — that invalidates the refresh token and
Sonic will ask you to reconnect on next launch.

---

## 7. Troubleshooting

**"Add your Spotify client id in Settings before connecting."**
You have not saved a Client ID yet. See step 2.

**"Spotify rejected the authorization code."**
The Client ID does not match the redirect URI registered for that app. Check both
character by character; the most common cause is a trailing space or a redirect
URI missing `/callback`.

**"No device found"**
Sonic controls playback, it does not play audio itself. Open the Spotify desktop
app or mobile app, start playing anything, and Sonic will see the device within a
second or two.

**Volume slider is greyed out**
The active device does not accept volume changes (common for TVs, consoles and
some speakers). Use that device's own volume control.

**Nothing happens when I click next**
Some devices are *restricted* — Spotify does not allow remote control of them.
Pick a different device from the device picker (the broadcast icon).

**The pill is invisible / see-through to clicks**
The window is intentionally transparent. If your desktop wallpaper is very light,
the glass may be hard to see. Raise **Settings → Appearance → Transparency** down
toward 0 and increase **Blur**.