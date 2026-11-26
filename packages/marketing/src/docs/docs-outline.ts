// A heading counts as the current section once its top passes this line, measured from the
// top of the window. It sits a little below the top so a section is marked as soon as it
// starts to fill the screen.
const CURRENT_SECTION_LINE_PX = 96;

/** Marks the "On this page" link for the section being read. */
export function setupDocsOutline() {
  const links = new Map(
    [...document.querySelectorAll<HTMLAnchorElement>("[data-outline-link]")].map((link) => [
      link.dataset.outlineLink,
      link,
    ]),
  );
  const headings = [...links.keys()].flatMap((id) => {
    const heading = id ? document.getElementById(id) : null;
    return heading ? [heading] : [];
  });
  if (!headings.length) return;

  let current: HTMLElement | undefined;
  let frame = 0;

  function update() {
    frame = 0;
    const atBottom =
      window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2;
    let next = headings[0];
    for (const heading of headings) {
      if (heading.getBoundingClientRect().top > CURRENT_SECTION_LINE_PX) break;
      next = heading;
    }
    if (atBottom) next = headings[headings.length - 1];
    if (next === current) return;
    if (current) links.get(current.id)?.removeAttribute("aria-current");
    links.get(next.id)?.setAttribute("aria-current", "true");
    current = next;
  }

  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(update);
  };
  window.addEventListener("scroll", schedule, { passive: true });
  window.addEventListener("resize", schedule);
  update();
}
