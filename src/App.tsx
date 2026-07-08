import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipc } from "@/lib/ipc";
import LoginForm from "@/components/LoginForm";
import TimerView from "@/components/TimerView";

type AuthState = "loading" | "signedOut" | "signedIn";

export default function App() {
  const [auth, setAuth] = useState<AuthState>("loading");

  // Initial gate: does a restored session exist?
  useEffect(() => {
    ipc
      .isAuthenticated()
      .then((ok) => setAuth(ok ? "signedIn" : "signedOut"))
      .catch(() => setAuth("signedOut"));

    // A 401 anywhere in the backend tears down the session and fires this.
    const unlisten = listen("auth:expired", () => setAuth("signedOut"));
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  // Persist window size (debounced), mirroring the Go app's resize handler.
  useEffect(() => {
    const win = getCurrentWindow();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unlisten = win.onResized(({ payload }) => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        ipc.saveWindowSize(payload.width, payload.height).catch(() => {});
      }, 400);
    });
    return () => {
      unlisten.then((off) => off());
      clearTimeout(timer);
    };
  }, []);

  if (auth === "loading") {
    return (
      <main className="flex h-full items-center justify-center bg-neutral-50 text-sm text-neutral-500 dark:bg-neutral-950">
        Loading…
      </main>
    );
  }

  if (auth === "signedOut") {
    return <LoginForm onAuthenticated={() => setAuth("signedIn")} />;
  }

  return <TimerView onSignOut={() => setAuth("signedOut")} />;
}
