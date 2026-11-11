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

  const connect = async (agent: string) => {
    setPending(agent);
    setError(null);
    try {
      await connectMcp(agent);
      setConnected(new Set(await getMcpConnections()));
    } catch (cause) {
      setError(invokeErrorMessage(cause));
    } finally {
      setPending(null);
    }
  };

  return { connected, pending, error, connect };
}
