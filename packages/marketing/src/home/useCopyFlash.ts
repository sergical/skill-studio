import { useEffect, useRef, useState } from "react";

/** Copies text to the clipboard; `copied` stays true for a moment after a copy succeeds. */
export function useCopyFlash() {
  const [copied, setCopied] = useState(false);
  const timer = useRef(0);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = (text: string) => {
    void navigator.clipboard?.writeText(text).then(
      () => {
        setCopied(true);
        window.clearTimeout(timer.current);
        timer.current = window.setTimeout(() => setCopied(false), 1500);
      },
      () => setCopied(false),
    );
  };
  return { copied, copy };
}
