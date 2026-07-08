/**
 * Wails-compatibility bridge.
 *
 * The UI was written against Wails' injected globals — `window.go.main.App.*`
 * (bindings) and `window.runtime.EventsOn/Off` (events). This shim provides
 * those same globals backed by Tauri's `invoke`/`listen`, so every component
 * runs unchanged.
 *
 * Two impedance mismatches are handled here, exactly as Wails + the Go client
 * did:
 *   - Wails returned camelCase (Go structs had camelCase json tags); the Tauri
 *     commands return the backend's snake_case. → results are snake→camel'd.
 *   - Object args from the UI are camelCase; the Rust serde models are
 *     snake_case. → object arg values are camel→snake'd.
 * Tauri already maps top-level arg *keys* (camelCase JS → snake_case Rust param),
 * so only nested object values need converting.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";

// --- key-case transforms (deep) -------------------------------------------

const toCamel = (s: string) =>
  s.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
const toSnake = (s: string) => s.replace(/[A-Z]/g, (c) => "_" + c.toLowerCase());

function deepKeys(value: unknown, keyFn: (k: string) => string): unknown {
  if (Array.isArray(value)) return value.map((v) => deepKeys(v, keyFn));
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[keyFn(k)] = deepKeys(v, keyFn);
    }
    return out;
  }
  return value; // scalars pass through untouched
}

const snakeToCamel = <T,>(v: unknown): T => deepKeys(v, toCamel) as T;
const camelToSnake = (v: unknown): unknown => deepKeys(v, toSnake);

/** Invoke a command, normalizing the result to camelCase and errors to Error. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return snakeToCamel<T>(await invoke(cmd, args));
  } catch (e: unknown) {
    // Tauri rejects with the serialized AppError { code, message }; the UI's
    // friendlyError() expects a string/Error, matching Wails' string errors.
    const msg =
      typeof e === "string"
        ? e
        : (e as { message?: string })?.message ?? String(e);
    throw new Error(msg);
  }
}

// --- window.go.main.App ----------------------------------------------------

const App = {
  Login: (email: string, password: string) =>
    call("login", { email, password }),
  SetNewPassword: (newPassword: string) =>
    call("set_new_password", { newPassword }),
  Logout: () => call<void>("logout"),
  SignIn: (data: unknown) =>
    call("sign_in", { data: camelToSnake(data) }),
  SignOut: () => call("sign_out"),
  GetMyAttendance: () => call("get_my_attendance"),
  GetMyTasks: () => call("get_my_tasks"),
  GetCurrentUser: () => call("get_current_user"),
  ShowWindow: () => call<void>("show_window"),
  CheckForUpdate: () => call("check_for_update"),
  InstallUpdate: () => call<void>("install_update"),
  GetAppVersion: () => call<string>("get_app_version"),
  GetWebDashboardURL: () => call<string>("get_web_dashboard_url"),
  SetAutoStart: (enabled: boolean) => call<void>("set_auto_start", { enabled }),
  GetAutoStart: () => call<boolean>("get_auto_start"),
  ClearLocalCache: () => call<void>("clear_local_cache"),
  ShowTrayNotification: (title: string, message: string) =>
    call<void>("show_tray_notification", { title, message }),
  SaveWindowSize: (width: number, height: number) =>
    call<void>("save_window_size", { width, height }),
  GetIdleSeconds: () => call<number>("get_idle_seconds"),
  GetSessionInfo: () => call("get_session_info"),
};

// --- window.runtime (events) ----------------------------------------------

const listeners = new Map<string, Promise<UnlistenFn>[]>();

const runtime = {
  EventsOn(event: string, callback: (...args: unknown[]) => void) {
    const p = listen(event, (e) => callback(snakeToCamel(e.payload)));
    const arr = listeners.get(event) ?? [];
    arr.push(p);
    listeners.set(event, arr);
  },
  EventsOff(event: string) {
    const arr = listeners.get(event);
    if (arr) {
      arr.forEach((p) => p.then((un) => un()).catch(() => {}));
      listeners.set(event, []);
    }
  },
  EventsEmit(event: string, data?: unknown) {
    void emit(event, data);
  },
};

// Install the globals before any component renders.
(window as unknown as { go: unknown }).go = { main: { App } };
(window as unknown as { runtime: unknown }).runtime = runtime;
