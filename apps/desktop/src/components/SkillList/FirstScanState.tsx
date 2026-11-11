import { Button } from "@skill-studio/ui";

export interface FirstScanStateProps {
  isLoading?: boolean;
  error?: string | null;
  onRetry?: () => void;
}

export function FirstScanState({ isLoading, error, onRetry }: FirstScanStateProps) {
  if (isLoading || !error) {
    return (
      <p role="status" className="text-body text-text-tertiary">
        Loading skills…
      </p>
    );
  }
  return (
    <div className="flex flex-col items-start gap-3">
      <p role="alert" className="text-body text-text-tertiary">
        {error}
      </p>
      <Button onClick={onRetry}>Try again</Button>
    </div>
  );
}
