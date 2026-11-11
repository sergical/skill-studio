// ============================================================================
// Skill Studio - AppErrorBoundary
// The last-resort screen when a view throws while rendering: names the error,
// offers Reload, and keeps the stack one click away for a bug report.
// ============================================================================

import React from "react";
import { Button } from "@skill-studio/ui";

interface AppErrorBoundaryProps {
  children: React.ReactNode;
}

interface AppErrorBoundaryState {
  error: Error | null;
}

export class AppErrorBoundary extends React.Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error };
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <div className="flex h-full items-center justify-center bg-bg-primary p-10 text-text-primary">
        <div className="flex w-full max-w-lg flex-col gap-4">
          <h1 className="m-0 text-xl font-semibold">Skill Studio hit an error</h1>
          <p
            role="alert"
            className="m-0 select-text text-body text-text-secondary break-words whitespace-pre-wrap"
          >
            {error.message || "This screen stopped while it was drawing."}
          </p>
          <div>
            <Button onClick={() => window.location.reload()}>Reload Skill Studio</Button>
          </div>
          {error.stack && (
            <details className="text-small text-text-tertiary">
              <summary className="cursor-default">Details for a bug report</summary>
              <pre className="mt-2 max-h-64 overflow-auto font-mono text-caption break-words whitespace-pre-wrap select-text">
                {error.stack}
              </pre>
            </details>
          )}
        </div>
      </div>
    );
  }
}
