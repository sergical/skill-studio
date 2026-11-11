import { z } from "zod";

const commandFailure = z.object({ code: z.string(), message: z.string() });

export class SkillCommandError extends Error {
  constructor(
    message: string,
    readonly code: string,
  ) {
    super(message);
  }
}

export function normalizeCommandError(cause: unknown): Error {
  if (cause instanceof Error) return cause;
  const message = String(cause);
  if (message.startsWith("{")) {
    try {
      const parsed = commandFailure.safeParse(JSON.parse(message));
      if (parsed.success) return new SkillCommandError(parsed.data.message, parsed.data.code);
    } catch {
      return new Error(message);
    }
  }
  return new Error(message);
}
