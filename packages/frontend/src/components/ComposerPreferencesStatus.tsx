import { useEffect, useState } from "react";
import { CircleAlert, LoaderCircle } from "lucide-react";
import type { ConfigOptionsCatalog } from "@openaide/app-shell-contracts";

/** Session initialization remains visible independently of the editable draft. */
export function ComposerPreferencesStatus({ preferences, onResolve }: {
  preferences: ConfigOptionsCatalog["preferences"];
  onResolve?: (action: "retry" | "useCurrentSettings") => Promise<void>;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState(false);
  const noticeKey = preferences?.state === "settled" ? preferences.skippedCount : undefined;
  const [noticeExpired, setNoticeExpired] = useState(false);
  useEffect(() => {
    setNoticeExpired(false);
    if (!noticeKey) return;
    const timer = setTimeout(() => setNoticeExpired(true), 10_000);
    return () => clearTimeout(timer);
  }, [noticeKey]);
  if (!preferences) return null;
  const failed = preferences.state === "failed";
  const applying = preferences.state === "applying";
  if (!failed && !applying && (preferences.skippedCount === 0 || noticeExpired)) return null;
  const resolve = async (action: "retry" | "useCurrentSettings") => {
    if (!onResolve || pending) return;
    setPending(true);
    setError(false);
    try { await onResolve(action); }
    catch { setError(true); }
    finally { setPending(false); }
  };
  return (
    <div className={`composer-preferences-status composer-footer-status${failed ? " error" : ""}`} role={failed ? "alert" : "status"} aria-live="polite">
      {applying ? <LoaderCircle aria-hidden size={13} /> : failed ? <CircleAlert aria-hidden size={13} /> : null}
      <span>{applying ? "Applying your preferences…" : failed ? "Couldn’t apply your preferences." : "Some saved preferences are unavailable. Using current settings."}</span>
      {failed && onResolve ? <>
        <button disabled={pending} onClick={() => { void resolve("retry"); }} type="button">Retry</button>
        <button disabled={pending} onClick={() => { void resolve("useCurrentSettings"); }} type="button">Use current settings</button>
      </> : null}
      {error ? <span>Couldn’t complete that action. Try again.</span> : null}
    </div>
  );
}
