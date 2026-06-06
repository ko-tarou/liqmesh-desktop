/**
 * Local device identity, persisted to `localStorage`.
 *
 * `myId` is a stable per-device UUID generated once on first launch and never
 * regenerated (the BLE contract binds `senderId` to a connection trust-on-first-
 * use, so a churning id would break anti-spoofing and read receipts). The
 * display name is user-editable and starts empty.
 */

import { useCallback, useEffect, useState } from "react";

export const DEVICE_ID_KEY = "liqmesh-device-id";
export const DISPLAY_NAME_KEY = "liqmesh-display-name";

/** Read the existing device id, or mint and persist a fresh one. */
function loadOrCreateDeviceId(): string {
  const existing = localStorage.getItem(DEVICE_ID_KEY);
  if (existing && existing.length > 0) return existing;
  const id = crypto.randomUUID();
  localStorage.setItem(DEVICE_ID_KEY, id);
  return id;
}

export type Identity = {
  /** Stable device id (never changes for the lifetime of this install). */
  myId: string;
  /** User-editable display name (may be empty until the user sets it). */
  myName: string;
  /** Update and persist the display name. */
  setMyName: (name: string) => void;
};

export function useIdentity(): Identity {
  // Lazy init so the id is generated exactly once, synchronously on mount.
  const [myId] = useState<string>(loadOrCreateDeviceId);
  const [myName, setMyNameState] = useState<string>(
    () => localStorage.getItem(DISPLAY_NAME_KEY) ?? "",
  );

  useEffect(() => {
    localStorage.setItem(DISPLAY_NAME_KEY, myName);
  }, [myName]);

  const setMyName = useCallback((name: string) => setMyNameState(name), []);

  return { myId, myName, setMyName };
}
