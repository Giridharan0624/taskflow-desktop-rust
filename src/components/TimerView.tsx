import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ipc,
  ipcMessage,
  type Attendance,
  type SessionInfo,
  type Task,
  type User,
} from "@/lib/ipc";

interface Props {
  onSignOut: () => void;
}

/**
 * M2 timer view: shows the signed-in user, a task selector, and sign-in /
 * sign-out against the live API. The rich timer clock, meeting mode, and idle
 * prompt arrive with the monitor work (M3/M4).
 */
export default function TimerView({ onSignOut }: Props) {
  const [user, setUser] = useState<User | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [attendance, setAttendance] = useState<Attendance | null>(null);
  const [selectedTask, setSelectedTask] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [idle, setIdle] = useState(0);
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);
  const [updateNote, setUpdateNote] = useState<string | null>(null);
  const [autoStart, setAutoStart] = useState(false);

  const signedIn = attendance?.status === "SIGNED_IN";

  useEffect(() => {
    // Initial load. get_current_user is the authoritative auth probe.
    (async () => {
      try {
        const [u, a, t, s] = await Promise.all([
          ipc.getCurrentUser(),
          ipc.getMyAttendance(),
          ipc.getMyTasks(),
          ipc.getSessionInfo(),
        ]);
        setUser(u);
        setAttendance(a);
        setTasks(t);
        setSession(s);
      } catch (e) {
        setError(ipcMessage(e));
      }
    })();

    // Current autostart state for the toggle.
    ipc.getAutoStart().then(setAutoStart).catch(() => {});

    // Backend-pushed events.
    const offs = [
      listen<Attendance>("attendance:updated", (e) => setAttendance(e.payload)),
      listen<{ version: string }>("update:available", (e) =>
        setUpdateVersion(e.payload.version),
      ),
      listen<{ message: string }>("update:package-managed", (e) =>
        setUpdateNote(e.payload.message),
      ),
    ];
    return () => {
      offs.forEach((p) => p.then((off) => off()));
    };
  }, []);

  async function toggleAutoStart() {
    const next = !autoStart;
    setAutoStart(next);
    try {
      await ipc.setAutoStart(next);
    } catch {
      setAutoStart(!next); // revert on failure
    }
  }

  async function installUpdate() {
    try {
      await ipc.installUpdate();
    } catch (e) {
      setError(ipcMessage(e));
    }
  }

  // Poll idle seconds while the timer runs (drives the idle hint).
  useEffect(() => {
    if (!signedIn) {
      setIdle(0);
      return;
    }
    const id = setInterval(() => {
      ipc.getIdleSeconds().then(setIdle).catch(() => {});
    }, 3000);
    return () => clearInterval(id);
  }, [signedIn]);

  async function handleSignIn(e: React.FormEvent) {
    e.preventDefault();
    const task = tasks.find((t) => t.task_id === selectedTask);
    if (!task) {
      setError("Pick a task first.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const a = await ipc.signIn({
        task_id: task.task_id,
        project_id: task.project_id,
        task_title: task.title,
        project_name: task.project_name ?? "",
        description,
      });
      setAttendance(a);
      setDescription("");
    } catch (err) {
      setError(ipcMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleStop() {
    setBusy(true);
    setError(null);
    try {
      const a = await ipc.signOut();
      setAttendance(a);
    } catch (err) {
      setError(ipcMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleLogout() {
    try {
      await ipc.logout();
      onSignOut();
    } catch (e) {
      setError(ipcMessage(e));
    }
  }

  const inputClass =
    "w-full rounded-md border border-neutral-300 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-900";

  return (
    <main className="flex h-full flex-col gap-4 bg-neutral-50 p-5 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <header className="flex items-center justify-between">
        <div>
          <div className="text-sm font-semibold">{user?.name ?? "…"}</div>
          <div className="text-xs text-neutral-500">{user?.email}</div>
        </div>
        <button
          onClick={handleLogout}
          className="text-xs text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200"
        >
          Sign out
        </button>
      </header>

      <div className="rounded-lg border border-neutral-200 p-4 dark:border-neutral-800">
        <div className="text-xs uppercase tracking-wide text-neutral-400">Status</div>
        <div
          className={`mt-1 text-lg font-medium ${
            signedIn ? "text-green-600 dark:text-green-400" : "text-neutral-500"
          }`}
        >
          {signedIn ? "Timer running" : "Not tracking"}
        </div>
        {signedIn && attendance?.current_task && (
          <div className="mt-1 text-sm text-neutral-600 dark:text-neutral-300">
            {attendance.current_task.task_title}
          </div>
        )}
        <div className="mt-2 text-xs text-neutral-400">
          Today: {attendance ? attendance.total_hours.toFixed(2) : "0.00"} h
        </div>
        {signedIn && idle >= 60 && (
          <div className="mt-1 text-xs text-amber-600 dark:text-amber-400">
            Idle for {Math.floor(idle / 60)}m {idle % 60}s
          </div>
        )}
      </div>

      {session?.limitation && (
        <p className="rounded-md bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:bg-amber-950/40 dark:text-amber-300">
          {session.limitation}
        </p>
      )}

      {updateVersion && (
        <div className="flex items-center justify-between rounded-md bg-blue-50 px-3 py-2 text-xs text-blue-700 dark:bg-blue-950/40 dark:text-blue-300">
          <span>Update {updateVersion} available</span>
          <button
            onClick={installUpdate}
            className="rounded bg-blue-600 px-2 py-1 text-xs font-medium text-white hover:bg-blue-700"
          >
            Install &amp; restart
          </button>
        </div>
      )}

      {updateNote && (
        <p className="rounded-md bg-blue-50 px-3 py-2 text-xs text-blue-700 dark:bg-blue-950/40 dark:text-blue-300">
          {updateNote}
        </p>
      )}

      {signedIn ? (
        <button
          onClick={handleStop}
          disabled={busy}
          className="w-full rounded-md bg-red-600 px-3 py-2 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
        >
          {busy ? "Stopping…" : "Stop timer"}
        </button>
      ) : (
        <form onSubmit={handleSignIn} className="flex flex-col gap-3">
          <select
            value={selectedTask}
            onChange={(e) => setSelectedTask(e.target.value)}
            className={inputClass}
          >
            <option value="">Select a task…</option>
            {tasks.map((t) => (
              <option key={t.task_id} value={t.task_id}>
                {t.title}
                {t.project_name ? ` · ${t.project_name}` : ""}
              </option>
            ))}
          </select>
          <input
            type="text"
            placeholder="What are you working on?"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            className={inputClass}
          />
          <button
            type="submit"
            disabled={busy}
            className="w-full rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            {busy ? "Starting…" : "Start timer"}
          </button>
        </form>
      )}

      {error && <p className="text-sm text-red-500">{error}</p>}

      <label className="mt-auto flex items-center gap-2 text-xs text-neutral-500">
        <input
          type="checkbox"
          checked={autoStart}
          onChange={toggleAutoStart}
          className="h-3.5 w-3.5"
        />
        Launch at login
      </label>
    </main>
  );
}
