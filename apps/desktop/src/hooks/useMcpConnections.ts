import { useCallback, useEffect, useState } from "react";
import { connectMcp, getMcpConnections, invokeErrorMessage } from "../lib/skill-api";

export interface McpConnections {
  /** Agent ids that already declare the server; `null` until the first read returns. */
  connected: ReadonlySet<string> | null;
  /** The agent whose connect is running. */
  pending: string | null;
  /** The agents whose connect finished, so a row can show what to do next. */
  attempted: ReadonlySet<string>;
  /** First line: a sentence for the user. The rest: the command and its output. */
  error: string | null;
  connect: (agent: string) => Promise<void>;
}

export function useMcpConnections(): McpConnections {
  const [connected, setConnected] = useState<ReadonlySet<string> | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [attempted, setAttempted] = useState<ReadonlySet<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(
    () =>
      getMcpConnections().then((agents) => {
        setConnected(new Set(agents));
      }),
    [],
  );

  useEffect(() => {
    let cancelled = false;
    getMcpConnections()
      .then((agents) => {
        if (!cancelled) setConnected(new Set(agents));
      })
      .catch((cause) => {
        if (!cancelled) setError(invokeErrorMessage(cause));
      });
    // Cursor writes its entry after its own install dialog, and a user can add
    // the server from a terminal, so read again when the window comes back.
    const onFocus = () => void reload().catch(() => undefined);
    window.addEventListener("focus", onFocus);
    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
    };
  }, [reload]);

  const connect = (agent: string) => {
    setPending(agent);
    setError(null);
    return (
      connectMcp(agent)
        .then(() => null, invokeErrorMessage)
        // Reload after a failure too: a re-add can remove the old entry before its add fails.
        .then((failure) =>
          reload().then(
            () => failure,
            (cause) => failure ?? invokeErrorMessage(cause),
          ),
        )
        .then((message) => {
          setError(message);
          setPending(null);
          setAttempted((previous) => new Set(previous).add(agent));
        })
    );
  };

  return { connected, pending, attempted, error, connect };
}
