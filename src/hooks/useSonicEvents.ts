import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { usePlayer } from "../store/playerStore";
import { EVENTS, type UiState } from "../types/spotify";

/**
 * Bridges backend events into the store.
 *
 * The Rust side is authoritative: it emits a full `UiState` after every poll and
 * after every command. We never compute playback state here — we only render
 * what we were told, plus short-lived optimistic overrides.
 */
export function useSonicEvents() {
  const applyState = usePlayer((s) => s.applyState);
  const openSettings = usePlayer((s) => s.openSettings);
  const setPanel = usePlayer((s) => s.setPanel);

  // The last clipboard revision already loaded into the store. Kept in a ref so
  // the listener reads the current value without re-subscribing on every
  // capture — a re-subscribe per copied snippet would be a listener leak.
  const seenRevision = useRef<number>(-1);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    (async () => {
      try {
        const unState = await listen<UiState>(EVENTS.state, (event) => {
          if (cancelled) return;
          applyState(event.payload);

          // A capture happened while Pocket is open: re-read the vault list.
          // Only the list is refreshed, and only when the revision actually
          // moved — the poller pushes a snapshot on every playback change too,
          // and re-reading the vault on those would be pointless disk IO.
          const revision = event.payload.settings?.clipboardRevision ?? 0;
          const store = usePlayer.getState();
          if (revision !== seenRevision.current) {
            seenRevision.current = revision;
            if (store.panel === "pocket") void store.refreshVault();
          }
        });

        // The backend signals that the refresh token is gone; the UI should stop
        // pretending it is connected and show the connect affordance.
        const unAuth = await listen<void>(EVENTS.authRequired, () => {
          if (cancelled) return;
          const current = usePlayer.getState();
          applyState({
            ...(current.state ?? ({} as UiState)),
            connected: false,
            authenticating: false,
            playback: null,
            error: {
              code: "unauthorized",
              message:
                "Spotify needs you to sign in again. Reconnect your account to keep controlling playback.",
              retryAfterSeconds: null,
            },
          });
        });

        // Tray menu → Settings.
        const unSettings = await listen<void>(EVENTS.openSettings, () => {
          if (!cancelled) {
            openSettings();
            setPanel("settings");
          }
        });

        if (cancelled) {
          unState();
          unAuth();
          unSettings();
          return;
        }
        unlisteners.push(unState, unAuth, unSettings);
      } catch (err) {
        // If the event bridge is unavailable the app still works: the initial
        // bootstrap snapshot is already in the store, and controls call commands
        // which return updated state. Warn, don't crash.
        console.warn("Sonic: event bridge unavailable", err);
      }
    })();

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [applyState, openSettings, setPanel]);
}