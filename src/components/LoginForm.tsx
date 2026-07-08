import { useState } from "react";
import { ipc, ipcMessage } from "@/lib/ipc";

interface Props {
  onAuthenticated: () => void;
}

/**
 * M1 login flow: email + password, with the NEW_PASSWORD_REQUIRED second step
 * for first-time sign-in. Mirrors the Go app's LoginForm behavior. The Cognito
 * challenge session stays in the Rust backend — this component only sends the
 * new password.
 */
export default function LoginForm({ onAuthenticated }: Props) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [stage, setStage] = useState<"credentials" | "newPassword">("credentials");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submitCredentials(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const result = await ipc.login(email, password);
      if (result.requiresNewPassword) {
        setStage("newPassword");
      } else {
        onAuthenticated();
      }
    } catch (err) {
      setError(ipcMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function submitNewPassword(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await ipc.setNewPassword(newPassword);
      onAuthenticated();
    } catch (err) {
      setError(ipcMessage(err));
    } finally {
      setBusy(false);
    }
  }

  const inputClass =
    "w-full rounded-md border border-neutral-300 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-neutral-700 dark:bg-neutral-900";
  const buttonClass =
    "w-full rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50";

  return (
    <div className="flex h-full flex-col items-center justify-center gap-6 bg-neutral-50 p-8 dark:bg-neutral-950">
      <div className="text-center">
        <h1 className="text-2xl font-semibold tracking-tight">TaskFlow</h1>
        <p className="text-sm text-neutral-500">Sign in to start tracking</p>
      </div>

      {stage === "credentials" ? (
        <form onSubmit={submitCredentials} className="flex w-full max-w-xs flex-col gap-3">
          <input
            type="email"
            placeholder="Email"
            autoComplete="username"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
            className={inputClass}
          />
          <input
            type="password"
            placeholder="Password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            className={inputClass}
          />
          <button type="submit" disabled={busy} className={buttonClass}>
            {busy ? "Signing in…" : "Sign in"}
          </button>
        </form>
      ) : (
        <form onSubmit={submitNewPassword} className="flex w-full max-w-xs flex-col gap-3">
          <p className="text-center text-xs text-neutral-500">
            Set a new password to finish activating your account.
          </p>
          <input
            type="password"
            placeholder="New password"
            autoComplete="new-password"
            value={newPassword}
            onChange={(e) => setNewPassword(e.target.value)}
            required
            className={inputClass}
          />
          <button type="submit" disabled={busy} className={buttonClass}>
            {busy ? "Saving…" : "Set password"}
          </button>
        </form>
      )}

      {error && <p className="max-w-xs text-center text-sm text-red-500">{error}</p>}
    </div>
  );
}
