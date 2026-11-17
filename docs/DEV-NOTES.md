# Dev-loop notes

Two things that look like crashes in the dev logs but are not, plus one real fix.

## `process didn't exit successfully: target\debug\sonic.exe (exit code: 1)`

**Not a crash.** This is what `cargo run` prints when the child is terminated
externally — every `taskkill /IM sonic.exe /F` produces it. Exit code 1 from a
force-kill is expected. The app itself does not print a panic or a Rust backtrace,
and the release binary starts and renders normally afterwards.

How to tell them apart:

- **Killed:** no output from the app at all, just the cargo epilogue.
- **Crashed:** a `thread 'main' panicked at ...` block, or a Rust backtrace.

If you see the second, that is a real bug; the first is just the dev loop ending.

## `Port 5183 is already in use`

**Caused by overlapping `tauri dev` runs**, not by the app. Each `tauri dev` starts
its own Vite server; starting a second one before the first has exited means the new
Vite cannot bind 5183. Because `strictPort` is `true` it fails loudly instead of
silently drifting to another port, which is the correct behaviour: Tauri's `devUrl`
points at 5183, so a silent fallback would load a stale or empty page.

If you hit it:

```bash
# find and kill whatever holds the port
netstat -ano | grep :5183
taskkill /PID <pid> /F
```

Or just let the running dev loop finish; it is already serving the app.

## `The file does not exist at ".../.vite/deps/@tauri-apps__api__core.js"` — fixed

This was real, and it was intermittent.

Vite's dependency pre-bundler rewrites `@tauri-apps/api` into
`node_modules/.vite/deps/`. After a config change or an HMR update, the page could
request a chunk that the optimizer had already replaced, and the IPC bridge failed
to load — which looks exactly like a broken app.

**Fix** — `vite.config.ts`:

```ts
optimizeDeps: {
  exclude: ["@tauri-apps/api"],
},
```

Tauri's API package is ESM that the WebView2 runtime loads directly; excluding it
keeps the import specifiers the Tauri IPC bridge expects. After clearing
`node_modules/.vite`, a clean `tauri dev` starts with no warning.

### One trap worth knowing

While diagnosing this I ran a CDP probe that did:

```js
await import('/node_modules/.vite/deps/@tauri-apps_api_core.js')
```

That is the *pre-bundled* path. Once the package is excluded from pre-bundling,
that file correctly does not exist — so the probe itself produced the exact
warning above, making it look like the fix had failed. No application source
references `.vite/deps` (verified with `grep -rn "vite/deps" src/`), so the app
never asks for it.

If you see the warning, check whether something is importing the pre-bundled path
directly before assuming the config regressed.

To reset the cache:

```bash
rm -rf node_modules/.vite
npm run tauri dev
```

## Clean restart recipe

```bash
taskkill /IM sonic.exe /F        # stop a running dev/release app
taskkill /IM node.exe /F         # stop stray vite servers
rm -rf node_modules/.vite        # only if you saw the deps warning
npm run tauri dev
```
