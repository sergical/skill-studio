import fc from "fast-check";
import { describe, expect, it } from "vitest";
import {
  contentForSave,
  isContentDirty,
  lineRange,
  normalizeLineEndings,
} from "./skill-editor-lines";

/** Text whose lines hold no line-end characters, plus any of LF, CRLF or a lone CR between them. */
const lineText = fc.string({ unit: fc.constantFrom("a", "b", " ", "-", ":", "é") });
const lineEnd = fc.constantFrom("\n", "\r\n", "\r");
const rawText = fc
  .array(fc.tuple(lineText, lineEnd), { maxLength: 6 })
  .chain((pairs) => lineText.map((last) => pairs.map(([text, end]) => text + end).join("") + last));

describe("lineRange", () => {
  it("line_range_selects_exactly_the_requested_line_in_the_lf_text_for_any_line_ending_or_jumps_to_the_wrong_row", () => {
    fc.assert(
      fc.property(rawText, fc.integer({ min: -2, max: 10 }), (raw, line) => {
        const lines = normalizeLineEndings(raw).split("\n");
        const range = lineRange(raw, line);
        if (line < 1 || line > lines.length) {
          expect(range).toBeNull();
          return;
        }
        const start = lines.slice(0, line - 1).reduce((sum, text) => sum + text.length + 1, 0);
        expect(range).toEqual({ start, end: start + lines[line - 1].length });
      }),
    );
  });

  it("line_range_returns_null_for_a_fractional_line_or_selects_garbage", () => {
    expect(lineRange("a", 1.5)).toBeNull();
  });
});

describe("isContentDirty", () => {
  it("dirty_compare_treats_a_crlf_file_as_clean_after_an_edit_is_undone_or_stays_dirty", () => {
    const crlf = "a\r\nb\r\n";
    expect(isContentDirty("a\nb\n", crlf)).toBe(false);
    expect(isContentDirty("a\nb\nx", crlf)).toBe(true);
  });

  it("dirty_compare_treats_a_crlf_file_as_clean_when_the_editor_opens_or_cmd_s_rewrites_it", () => {
    const crlf = "a\r\nb\r\n";
    expect(isContentDirty(normalizeLineEndings(crlf), crlf)).toBe(false);
  });
});

describe("normalizeLineEndings", () => {
  it("normalize_turns_a_lone_carriage_return_into_a_newline_or_line_numbers_drift", () => {
    expect(normalizeLineEndings("a\rb\r\nc")).toBe("a\nb\nc");
  });
});

describe("contentForSave", () => {
  it("save_content_gives_back_a_crlf_or_lf_file_byte_for_byte_after_open_and_save_or_rewrites_its_line_ends", () => {
    fc.assert(
      fc.property(fc.array(lineText, { maxLength: 6 }), fc.boolean(), (lines, crlf) => {
        const raw = lines.join(crlf ? "\r\n" : "\n");
        expect(contentForSave(normalizeLineEndings(raw), raw)).toBe(raw);
      }),
    );
  });

  it("save_content_writes_only_crlf_for_a_crlf_file_whatever_the_editor_holds_or_doubles_a_carriage_return", () => {
    fc.assert(
      fc.property(rawText, (content) => {
        const saved = contentForSave(content.replace(/\r(?!\n)/g, "\n"), "x\r\ny");
        expect(saved).not.toMatch(/\r\r|\r(?!\n)|(?<!\r)\n/);
      }),
    );
  });
});
