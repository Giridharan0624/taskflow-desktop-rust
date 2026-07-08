import { invoke } from "@tauri-apps/api/core";

export interface LoginResult {
  requiresNewPassword: boolean;
}

// API models — snake_case to match the backend wire format the Rust layer
// deserializes and re-serializes verbatim.
export interface User {
  user_id: string;
  email: string;
  name: string;
  system_role: string;
  department: string | null;
  avatar_url: string | null;
  employee_id: string | null;
  skills: string[];
}

export interface Task {
  task_id: string;
  project_id: string;
  title: string;
  description: string | null;
  status: string;
  priority: string;
  domain: string;
  assigned_to: string[];
  deadline: string;
  project_name?: string | null;
}

export interface CurrentTask {
  task_id: string;
  project_id: string;
  task_title: string;
  project_name: string;
}

export interface Attendance {
  user_id: string;
  date: string;
  sessions: unknown[];
  total_hours: number;
  current_sign_in_at: string | null;
  current_task: CurrentTask | null;
  user_name: string;
  user_email: string;
  system_role: string;
  status: string; // "SIGNED_IN" | "SIGNED_OUT"
  session_count: number;
  server_time?: string;
}

export interface StartTimerData {
  task_id: string;
  project_id: string;
  task_title: string;
  project_name: string;
  description: string;
}

export interface SessionInfo {
  canTrackWindows: boolean;
  displayServer: string;
  limitation: string | null;
}

export interface UpdateInfo {
  available: boolean;
  version: string;
  currentVersion: string;
  downloadUrl: string;
  releaseNotes: string;
  fileName: string;
  size: number;
}

/**
 * Typed wrapper over Tauri's `invoke`. Replaces the Wails-injected
 * `window.go.main.App.*` bindings. One method per backend `#[command]`.
 */
export const ipc = {
  // --- M0: system ---
  getAppVersion: () => invoke<string>("get_app_version"),
  getWebDashboardURL: () => invoke<string>("get_web_dashboard_url"),
  showWindow: () => invoke<void>("show_window"),

  // --- M1: auth ---
  login: (email: string, password: string) =>
    invoke<LoginResult>("login", { email, password }),
  setNewPassword: (newPassword: string) =>
    invoke<void>("set_new_password", { newPassword }),
  logout: () => invoke<void>("logout"),
  isAuthenticated: () => invoke<boolean>("is_authenticated"),

  // --- M2: data + timer ---
  getCurrentUser: () => invoke<User>("get_current_user"),
  getMyTasks: () => invoke<Task[]>("get_my_tasks"),
  getMyAttendance: () => invoke<Attendance>("get_my_attendance"),
  signIn: (data: StartTimerData) => invoke<Attendance>("sign_in", { data }),
  signOut: () => invoke<Attendance>("sign_out"),

  // --- M3: tray + window ---
  showTrayNotification: (title: string, message: string) =>
    invoke<void>("show_tray_notification", { title, message }),
  saveWindowSize: (width: number, height: number) =>
    invoke<void>("save_window_size", { width, height }),

  // --- M4: activity monitor ---
  getIdleSeconds: () => invoke<number>("get_idle_seconds"),
  getSessionInfo: () => invoke<SessionInfo>("get_session_info"),

  // --- M5: offline cache ---
  clearLocalCache: () => invoke<void>("clear_local_cache"),

  // --- M6: updater + autostart ---
  checkForUpdate: () => invoke<UpdateInfo>("check_for_update"),
  installUpdate: () => invoke<void>("install_update"),
  getAutoStart: () => invoke<boolean>("get_auto_start"),
  setAutoStart: (enabled: boolean) =>
    invoke<void>("set_auto_start", { enabled }),
};

/** Error shape serialized by the Rust `AppError`. */
export interface IpcError {
  code: "not_authenticated" | "unauthorized" | "network" | "error";
  message: string;
}

/** Narrow an unknown thrown value to a readable message. */
export function ipcMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as IpcError).message);
  }
  return String(e);
}
