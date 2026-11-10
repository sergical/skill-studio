// ============================================================================
// skill-editor-lines - pure line helpers for SkillMarkdownEditor's gutter
// ============================================================================

/**
 * Character range of the 1-based `line` in `content`, newline excluded, or
 * `null` when the text has no such line. Takes raw file text (CRLF allowed) and
 * returns offsets into its LF form, which is what the textarea holds.
 */
export function lineRange(raw: string, line: number): { start: number; end: number } | null {
  const content = normalizeLineEndings(raw);
  if (!Number.isInteger(line) || line < 1) return null;
  let start = 0;
  for (let current = 1; current < line; current += 1) {
    const newline = content.indexOf("\n", start);
    if (newline === -1) return null;
    start = newline + 1;
  }
  const newline = content.indexOf("\n", start);
  return { start, end: newline === -1 ? content.length : newline };
}

/** A textarea's value always uses LF, so offsets into a CRLF file only line up after this. */
export function normalizeLineEndings(content: string): string {
  return content.replace(/\r\n?/g, "\n");
}

/** Whether the textarea's LF `content` differs from the file as it was read (raw, maybe CRLF). */
export function isContentDirty(content: string, initialRaw: string): boolean {
  return content !== normalizeLineEndings(initialRaw);
}

/**
 * The text to write: `content` with CRLF restored when the original file used it.
 * A file that mixed CRLF and LF is written back as all CRLF; lone-CR line ends come back as LF.
 */
export function contentForSave(content: string, initialRaw: string): string {
  return initialRaw.includes("\r\n") ? content.replace(/\r?\n/g, "\r\n") : content;
}

/** The frontmatter through its closing `---` line, or all of `content` when it has none. */
function frontmatterBlock(content: string): string {
  return /^---\n[\s\S]*?\n---(?:\n|$)/.exec(content)?.[0] ?? content;
}

/**
 * Whether the YAML diagnostic for 1-based `line` still applies: the frontmatter
 * reads the same in the textarea's LF `content` as in the file as read, and the
 * line still exists. A YAML error can come from a line other than the one the
 * parser names, so any frontmatter edit clears the diagnostic.
 */
export function isYamlDiagnosticCurrent(
  content: string,
  initialRaw: string,
  line: number,
): boolean {
  return (
    lineRange(content, line) !== null &&
    frontmatterBlock(content) === frontmatterBlock(normalizeLineEndings(initialRaw))
  );
}
