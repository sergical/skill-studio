/** Wires the prerendered docs search to the existing navigation links. */
export function setupDocsSearch() {
  const input = document.querySelector<HTMLInputElement>("#docs-search");
  const form = input?.form;
  const nav = input?.closest("nav");
  if (!input || !form || !nav) return;

  const groups = [...nav.querySelectorAll<HTMLElement>("[data-docs-group]")].map((group) => ({
    group,
    links: [...group.querySelectorAll<HTMLAnchorElement>("[data-docs-headings]")].map((link) => ({
      link,
      searchText:
        `${link.dataset.docsTitle} ${link.textContent} ${link.dataset.docsHeadings}`.toLowerCase(),
    })),
  }));
  const toggle = nav.querySelector<HTMLButtonElement>(".docs-nav-toggle");
  const empty = nav.querySelector<HTMLElement>("[data-docs-search-empty]");

  const setBrowsing = (expanded: boolean) => {
    if (!toggle) return;
    const opening = expanded && !nav.hasAttribute("data-docs-browsing");
    toggle.setAttribute("aria-expanded", String(expanded));
    nav.toggleAttribute("data-docs-browsing", expanded);
    // On small screens the list starts closed; show the current page when it opens.
    if (opening) nav.querySelector("[aria-current=page]")?.scrollIntoView({ block: "nearest" });
  };

  let results: HTMLAnchorElement[] = [];
  function updateResults(query: string) {
    const words = query.trim().toLowerCase().split(/\s+/);
    results = [];
    for (const { group, links } of groups) {
      let groupMatches = 0;
      for (const { link, searchText } of links) {
        const match = words.every((word) => searchText.includes(word));
        link.hidden = !match;
        if (!match) continue;
        groupMatches++;
        results.push(link);
      }
      group.hidden = groupMatches === 0;
    }
    if (empty) empty.hidden = results.length > 0;
  }

  updateResults(input.value);
  if (input.value.trim()) setBrowsing(true);

  toggle?.addEventListener("click", () => {
    setBrowsing(toggle.getAttribute("aria-expanded") === "false");
  });
  input.addEventListener("input", () => {
    if (input.value.trim()) setBrowsing(true);
    updateResults(input.value);
  });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (input.value.trim()) results[0]?.click();
  });

  nav.addEventListener("keydown", (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;
    const link = target.closest<HTMLAnchorElement>("[data-docs-headings]");
    if (target !== input && !link) return;
    if (event.key === "Escape" && input.value) {
      event.preventDefault();
      input.focus();
      input.value = "";
      updateResults("");
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    if (!results.length) return;

    event.preventDefault();
    setBrowsing(true);
    const step = event.key === "ArrowDown" ? 1 : -1;
    const index = link ? results.indexOf(link) : step === 1 ? -1 : results.length;
    const next = results[index + step];
    if (next) next.focus();
    else input.focus();
  });

  document.addEventListener("keydown", (event) => {
    if (
      event.defaultPrevented ||
      event.key !== "/" ||
      event.metaKey ||
      event.ctrlKey ||
      event.altKey ||
      !(event.target instanceof Element) ||
      event.target.closest("input, textarea, select, [contenteditable], [role=menu]")
    ) {
      return;
    }
    event.preventDefault();
    input.focus();
  });
}
