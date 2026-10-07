import { fileURLToPath } from "node:url";
import stylex from "@stylexjs/unplugin";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin, type ViteDevServer } from "vite";
import { PAGE_TEMPLATES } from "./src/prerender/site-templates.ts";

const PRERENDER_ENTRY = "/src/prerender/site-prerender.tsx";

const pageInputs = Object.fromEntries(
  PAGE_TEMPLATES.map((file) => [
    file.replace(/\/?index\.html$|\.html$/, "") || "home",
    fileURLToPath(new URL(file, import.meta.url)),
  ]),
);

/**
 * Fills each page template on the dev server the same way scripts/prerender-docs.mjs does
 * after a build, so the theme script, meta tags and static docs markup match production.
 */
function devPrerender(): Plugin {
  // SAFETY: ssrLoadModule returns an untyped record; PRERENDER_ENTRY is this exact module.
  const loadPrerender = async (server: ViteDevServer) =>
    (await server.ssrLoadModule(
      PRERENDER_ENTRY,
    )) as typeof import("./src/prerender/site-prerender");

  return {
    name: "skill-studio-dev-prerender",
    apply: "serve",
    configureServer(server) {
      // The build writes the docs markdown copies and llms.txt to dist; serve them from source here.
      server.middlewares.use(async (req, res, next) => {
        const path = req.url?.split("?")[0].replace(/^\//, "");
        if (path !== "llms.txt" && !(path?.startsWith("docs/") && path.endsWith(".md")))
          return next();
        try {
          const prerender = await loadPrerender(server);
          if (path === "llms.txt") {
            res.setHeader("Content-Type", "text/plain; charset=utf-8");
            return res.end(prerender.renderLlmsText());
          }
          const route = prerender.SITE_ROUTES.find((entry) => entry.markdownFile === path);
          if (!route?.markdown) return next();
          res.setHeader("Content-Type", "text/markdown; charset=utf-8");
          res.end(route.markdown);
        } catch (error) {
          next(error);
        }
      });
    },
    transformIndexHtml: {
      order: "pre",
      async handler(html, ctx) {
        if (!ctx.server) return html;
        const file = ctx.path.replace(/^\//, "").replace(/(^|\/)$/, "$1index.html");
        return (await loadPrerender(ctx.server)).renderSitePage(file, html);
      },
    },
  };
}

export default defineConfig(({ isSsrBuild }) => ({
  plugins: [stylex.vite({ useCSSLayers: true }), react(), devPrerender()],
  build: isSsrBuild
    ? {}
    : {
        // The StyleX plugin appends every collected rule to a single CSS asset. With one
        // CSS file per page, the docs pages would miss the rules gathered from other entries.
        cssCodeSplit: false,
        rollupOptions: { input: pageInputs },
      },
}));
