import { useEffect, useRef, useState } from "react";

/**
 * Copies text to the clipboard; `copiedText` holds what was copied for a moment after a copy
 * succeeds, so a control that copies different text can tell whether its own text was copied.
 */
export function useCopyFlash() {
  const [copiedText, setCopiedText] = useState<string | null>(null);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = (text: string) => {
    void navigator.clipboard?.writeText(text).then(
      () => {
        setCopiedText(text);
        window.clearTimeout(timer.current);
        timer.current = window.setTimeout(() => setCopiedText(null), 1500);
      },
      () => setCopiedText(null),
    );
  };
  return { copied: copiedText !== null, copiedText, copy };
}
