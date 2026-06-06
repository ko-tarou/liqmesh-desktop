/**
 * Launch-time Bluetooth precheck.
 *
 * Invokes the Rust `ble_available` command once on mount so the UI can prompt
 * the user to enable Bluetooth instead of letting a later scan silently fail.
 * Re-checkable via `recheck()` (e.g. after the user flips Bluetooth on and taps
 * "retry"). While the first probe is in flight `available` is `null` (unknown);
 * we treat unknown as "don't nag yet".
 */

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Mirrors the Rust `BleAvailability` (camelCase). */
type BleAvailability = {
  available: boolean;
  reason: string | null;
};

export type UseBleAvailability = {
  /** true / false once probed; null while the first probe is in flight. */
  available: boolean | null;
  /** Human-readable reason when unavailable (else null). */
  reason: string | null;
  /** Re-run the probe (e.g. after the user enables Bluetooth). */
  recheck: () => Promise<void>;
};

export function useBleAvailability(): UseBleAvailability {
  const [available, setAvailable] = useState<boolean | null>(null);
  const [reason, setReason] = useState<string | null>(null);

  const recheck = useCallback(async () => {
    try {
      const res = await invoke<BleAvailability>("ble_available");
      setAvailable(res.available);
      setReason(res.reason);
    } catch (e) {
      // The command itself shouldn't throw, but if the bridge fails, treat it
      // as unavailable with the error as the reason rather than crashing the UI.
      setAvailable(false);
      setReason(String(e));
    }
  }, []);

  useEffect(() => {
    void recheck();
  }, [recheck]);

  return { available, reason, recheck };
}
