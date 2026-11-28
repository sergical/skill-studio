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

const PRIMITIVE_REJECTION = z.union([z.string(), z.number(), z.boolean(), z.bigint()]);

/** The text to show for a caught rejection. A command rejection is already an `Error` (see
 * `normalizeCommandError`), so this reads its message. */
export function invokeErrorMessage(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  // Other callers (a clipboard rejection, say) can pass anything: a primitive reads as itself,
  // and any other value as "Unknown error", not "[object Object]".
  const primitive = PRIMITIVE_REJECTION.safeParse(cause);
  return primitive.success ? String(primitive.data) : "Unknown error";
}
