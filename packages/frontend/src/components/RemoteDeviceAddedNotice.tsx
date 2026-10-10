import type { DeviceCollectionSnapshot, RemoteDeviceSummary } from "@openaide/app-server-client";
import { useEffect, useRef, useState } from "react";

/**
 * Tells every connected client when a Remote Device joins, so an unexpected pairing is noticed
 * and can be removed. The first list a client receives is its baseline and announces nothing.
 */
export function RemoteDeviceAddedNotice({
  devices,
  onReview,
}: {
  devices?: DeviceCollectionSnapshot;
  onReview: () => void;
}) {
  const known = useRef<Set<string>>(undefined);
  const [added, setAdded] = useState<RemoteDeviceSummary>();
  useEffect(() => {
    if (!devices) return;
    const current = devices.devices ?? [];
    const previous = known.current;
    known.current = new Set(current.map((device) => device.deviceId));
    if (!previous) return;
    const joined = current.find((device) => !previous.has(device.deviceId));
    if (joined) setAdded(joined);
    else setAdded((shown) => (shown && !known.current?.has(shown.deviceId) ? undefined : shown));
  }, [devices]);
  if (!added) return null;
  return (
    <aside className="device-added-notice" role="status">
      <div>
        <strong>New device added</strong>
        <p>{added.name} can now use this OpenAIDE{added.addedBy ? `, added from ${added.addedBy}` : ""}.</p>
      </div>
      <footer>
        <button onClick={() => setAdded(undefined)} type="button">Dismiss</button>
        <button onClick={() => { setAdded(undefined); onReview(); }} type="button">Review devices</button>
      </footer>
    </aside>
  );
}
