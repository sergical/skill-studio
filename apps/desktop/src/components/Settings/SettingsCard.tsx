// ============================================================================
// SettingsCard - the bordered, icon-headed shell every Settings card shares:
// an icon, a title, an optional action at the right of the heading row, one
// sentence describing what the card's setting does, then its content. Also
// the row, error and copy pieces the cards share, so every card lines up,
// sizes its buttons and reports a failure the same way.
// ============================================================================

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Copy, type LucideIcon } from "lucide-react";
import { Button } from "@skill-studio/ui";

interface SettingsCardProps {
  icon: LucideIcon;
  title: string;
  description: ReactNode;
  /** Rendered at the right of the heading row, e.g. an "Add folder…" button. */
  action?: ReactNode;
  children: ReactNode;
}

export function SettingsCard({
  icon: Icon,
  title,
  description,
  action,
  children,
}: SettingsCardProps) {
  return (
    <section className="flex flex-col gap-3 rounded-lg border border-border-subtle p-4">
      <div className="flex min-h-7 items-center gap-2">
        <h2 className="m-0 flex flex-1 items-center gap-2 text-body font-semibold text-text-primary">
          <Icon size={15} className="text-text-tertiary" aria-hidden="true" />
          {title}
        </h2>
        {action}
      </div>
      <p className="m-0 max-w-prose text-small text-text-tertiary">{description}</p>
      {children}
    </section>
  );
}

interface SettingsRowProps {
  icon?: ReactNode;
  label: ReactNode;
  /** Small text after the label, e.g. "Added" or "Not on PATH". */
  status?: ReactNode;
  /** The control at the right edge. */
  control?: ReactNode;
  /** Wraps the row in a `<label>`, so a click anywhere on it toggles the control. */
  asLabel?: boolean;
}

/** One line in a card's list: icon, label, status, then a control at the right edge. */
export function SettingsRow({ icon, label, status, control, asLabel = false }: SettingsRowProps) {
  const Row = asLabel ? "label" : "div";
  return (
    <li>
      <Row
        className={`flex h-9 items-center gap-2 px-2 text-body text-text-secondary ${asLabel ? "cursor-pointer" : ""}`}
      >
        {icon && <span className="flex size-4 items-center justify-center">{icon}</span>}
        <span className="min-w-0 flex-1 truncate text-text-primary">{label}</span>
        {status}
        {control}
      </Row>
    </li>
  );
}

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

interface CopyButtonProps {
  text: string;
  label: string;
}

/** Copies `text` and confirms in place for a moment. */
export function CopyButton({ text, label }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <Button
      variant="outline"
      size="sm"
      onClick={() => {
        void navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
      {copied ? "Copied" : label}
    </Button>
  );
}
