// ============================================================================
// SettingsCard - the error piece every Settings section shares, so each one
// reports a failure the same way.
// ============================================================================

interface SettingsErrorProps {
  /** The first line is the sentence to show; the rest is detail behind "Show details". */
  message: string;
}

export function SettingsError({ message }: SettingsErrorProps) {
  const [summary, ...rest] = message.split("\n");
  const detail = rest.join("\n").trim();
  return (
    <div role="alert" className="flex flex-col gap-1 rounded-md bg-error/10 px-3 py-2 text-small">
      <p className="m-0 text-error">{summary}</p>
      {detail && (
        <details className="text-text-secondary">
          <summary className="cursor-pointer select-none text-text-tertiary">Show details</summary>
          <pre className="m-0 mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed">
            {detail}
          </pre>
        </details>
      )}
    </div>
  );
}

/** Copies `text` and confirms in place for a moment. */
