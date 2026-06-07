import { Bluetooth } from "lucide-react";

type Props = {
  /** Optional technical reason from the Rust probe (shown small, secondary). */
  reason?: string | null;
  /** Re-run the availability probe after the user enables Bluetooth. */
  onRetry: () => void;
};

/**
 * Friendly launch-time prompt shown when no usable Bluetooth adapter is found,
 * so the user enables it in settings rather than hitting a silently-failing
 * scan. A modal overlay with a single "再確認" (re-check) action.
 */
export function BluetoothDialog({ reason, onRetry }: Props) {
  return (
    <div className="bt-overlay" role="dialog" aria-modal="true" aria-labelledby="bt-title">
      <div className="bt-dialog">
        <div className="bt-icon" aria-hidden="true">
          <Bluetooth size={40} color="var(--brand)" />
        </div>
        <h2 id="bt-title" className="bt-title">
          Bluetoothを設定でオンにしてください
        </h2>
        <p className="bt-body">
          LiqMesh は近くの端末と直接つながるために Bluetooth を使います。
          設定で Bluetooth をオンにしてから、再確認してください。
        </p>
        {reason && <p className="bt-reason">詳細: {reason}</p>}
        <button type="button" className="btn-primary bt-retry" onClick={onRetry}>
          再確認
        </button>
      </div>
    </div>
  );
}
