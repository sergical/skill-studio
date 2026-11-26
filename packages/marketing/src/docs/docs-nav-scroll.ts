// Every docs link loads a new page, which would reset the sidebar to its top. The sidebar's
// scroll position is saved when the page is left and restored before the next one paints.

const NAV_SCROLL_KEY = "skill-studio-docs-nav-scroll";

/** Inline script placed right after the sidebar list, so it runs before the first paint. */
export function docsNavScrollRestoreScript(): string {
  return `(()=>{try{const l=document.getElementById("docs-links"),y=Number(sessionStorage.getItem(${JSON.stringify(NAV_SCROLL_KEY)}));if(l&&y)l.scrollTop=y}catch{}})()`;
}

export function rememberDocsNavScroll() {
  const links = document.getElementById("docs-links");
  if (!links) return;
  window.addEventListener("pagehide", () => {
    try {
      sessionStorage.setItem(NAV_SCROLL_KEY, String(links.scrollTop));
    } catch {
      // Blocked storage only means the sidebar starts at the top on the next page.
    }
  });
}
