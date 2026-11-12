import { useEffect, useState } from "react";
import { connectMcp, getMcpConnections, invokeErrorMessage } from "../lib/skill-api";

export interface McpConnections {
  /** Agent ids that already declare the server; `null` until the first read returns. */
  connected: ReadonlySet<string> | null;
  /** The agent whose `mcp add` is running. */
  pending: string | null;
  error: string | null;
  connect: (agent: string) => Promise<void>;
}

export function useMcpConnections(): McpConnections {
  const [connected, setConnected] = useState<ReadonlySet<string> | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getMcpConnections()
      .then((agents) => {
        if (!cancelled) setConnected(new Set(agents));
      })
      .catch((cause) => {
        if (!cancelled) setError(invokeErrorMessage(cause));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const connect = (agent: string) => {
    setPending(agent);
    setError(null);
    return (
      connectMcp(agent)
        .then(() => null, invokeErrorMessage)
        // Reload after a failure too: a Claude Code re-add can remove the old entry before its add fails.
        .then((failure) =>
          getMcpConnections().then(
            (agents) => {
              setConnected(new Set(agents));
              return failure;
            },
            (cause) => failure ?? invokeErrorMessage(cause),
          ),
        )
        .then((message) => {
          setError(message);
          setPending(null);
        })
    );
  };

  return { connected, pending, error, connect };
}
