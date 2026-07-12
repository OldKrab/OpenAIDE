import { useRef } from "react";

/** Guards browser reads by logical owner instead of callback-object identity. */
export function useBrowserRequestOwnership(ownerKey: string) {
  const currentOwnerKey = useRef(ownerKey);
  const ownerEpoch = useRef(0);
  const latestRead = useRef(0);
  currentOwnerKey.current = ownerKey;

  return {
    beginLatestRead: () => {
      const read = ++latestRead.current;
      const owner = ownerKey;
      const epoch = ownerEpoch.current;
      return () => currentOwnerKey.current === owner
        && ownerEpoch.current === epoch
        && latestRead.current === read;
    },
    captureOwner: () => {
      const owner = ownerKey;
      const epoch = ownerEpoch.current;
      return () => currentOwnerKey.current === owner && ownerEpoch.current === epoch;
    },
    invalidateOwner: () => {
      ownerEpoch.current += 1;
      latestRead.current += 1;
    },
  };
}
