// ============================================================================
// TrustConfirmFooter - the "trust this repository?" footer, shared by the Add
// Skill sheet's own needs-trust phase and the Skill Store install flow
// (review round 2, B1): both pause on the same `ops::install` trust gate, so
// both show the same copy rather than each inventing their own.
// ============================================================================

import { Button } from "@skill-studio/ui";

export function TrustConfirmFooter({
  untrustedIdentity,
  trustBusy,
  onCancel,
  onTrustAndRetry,
}: {
  untrustedIdentity: string | undefined;
  trustBusy: boolean;
  onCancel: () => void;
  onTrustAndRetry: () => void;
}) {
  return (
    <div className="flex flex-col gap-3 border-t border-border px-5 py-4">
      <div className="flex flex-col gap-1">
        <p className="m-0 text-body font-medium text-text-primary">Trust this repository?</p>
        <p className="m-0 font-mono text-small break-all text-text-secondary">
          {untrustedIdentity ?? "this repository"}
        </p>
        <p className="m-0 text-caption text-text-tertiary">
          Skills from it can run code on your machine. Trust it only if you know who publishes it.
        </p>
      </div>
      <div className="flex justify-end gap-2">
        <Button
          variant="outline"
          className="h-(--control-height) rounded-md px-3.5 text-body font-medium"
          onClick={onCancel}
          disabled={trustBusy}
        >
          Close
        </Button>
        <Button
          className="h-(--control-height) rounded-md bg-accent-solid px-3.5 text-body font-medium text-text-on-accent hover:bg-accent-solid-hover"
          onClick={onTrustAndRetry}
          disabled={trustBusy}
        >
          Trust repository and retry
        </Button>
      </div>
    </div>
  );
}
