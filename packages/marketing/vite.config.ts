import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { sentryVitePlugin } from "@sentry/vite-plugin";
import stylex from "@stylexjs/unplugin";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin, type ViteDevServer } from "vite";
import { PAGE_TEMPLATES } from "./src/prerender/site-templates.ts";
import { DOWNLOAD_REDIRECTS } from "./src/site-links.ts";

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
        const rawPath = req.url?.split("?")[0] ?? "";
        const downloadTarget = DOWNLOAD_REDIRECTS.get(rawPath);
        if (downloadTarget) {
          res.statusCode = 302;
          res.setHeader("Location", downloadTarget);
          return res.end();
        }
        const path = rawPath.replace(/^\//, "");
        if (!path || (path !== "llms.txt" && path !== "docs.md" && !path.startsWith("docs/"))) {
          return next();
        }
        try {
          const prerender = await loadPrerender(server);
          if (path === "llms.txt") {
            res.setHeader("Content-Type", "text/plain; charset=utf-8");
            return res.end(prerender.renderLlmsText());
          }
          const markdownPath = path === "docs.md" ? "docs/index.md" : path;
          const file = path.endsWith("/") ? `${path}index.html` : path;
          const route = prerender.SITE_ROUTES.find(
            (entry) => entry.markdownFile === markdownPath || entry.file === file,
          );
          if (!route) return next();
          if (route.markdownFile === markdownPath) {
            res.setHeader("Content-Type", "text/markdown; charset=utf-8");
            return res.end(route.markdown);
          }
          // New docs URLs share the same shell. Pass the requested URL through
          // transformIndexHtml so its metadata and assets are resolved normally.
          const template = await readFile(new URL(route.template, import.meta.url), "utf8");
          const html = await server.transformIndexHtml(`/${route.file}`, template);
          res.setHeader("Content-Type", "text/html; charset=utf-8");
          res.end(html);
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

/**
 * Uploads the client build's source maps to Sentry and deletes them from dist, so the
 * Worker assets never serve them. Workers Builds holds SENTRY_AUTH_TOKEN; a build
 * without it makes no source maps.
 */
const sentryAuthToken = process.env.SENTRY_AUTH_TOKEN;

function sentrySourceMaps(): Plugin[] {
  if (!sentryAuthToken) return [];
  return sentryVitePlugin({
    org: "sergtech",
    project: "skill-studio-marketing",
    authToken: sentryAuthToken,
    release: { name: process.env.WORKERS_CI_COMMIT_SHA },
    sourcemaps: { filesToDeleteAfterUpload: ["./dist/**/*.map"] },
    telemetry: false,
    errorHandler(error) {
      throw error;
    },
  });
}

export default defineConfig(({ isSsrBuild }) => ({
  plugins: [
    stylex.vite({ useCSSLayers: true }),
    react(),
    devPrerender(),
    ...(isSsrBuild ? [] : sentrySourceMaps()),
  ],
  // Workers Builds sets these two variables; they are empty on a local build.
  define: {
    __SITE_RELEASE__: JSON.stringify(process.env.WORKERS_CI_COMMIT_SHA ?? ""),
    __SITE_BRANCH__: JSON.stringify(process.env.WORKERS_CI_BRANCH ?? ""),
    // Sentry's tree-shaking flags: no debug logging, and no replay of iframes, shadow DOM or
    // the compression worker (recordings are sent uncompressed for the envelope scrub).
    __SENTRY_DEBUG__: "false",
    __RRWEB_EXCLUDE_IFRAME__: "true",
    __RRWEB_EXCLUDE_SHADOW_DOM__: "true",
    __SENTRY_EXCLUDE_REPLAY_WORKER__: "true",
  },
  build: isSsrBuild
    ? {}
    : {
        // The StyleX plugin appends every collected rule to a single CSS asset. With one
        // CSS file per page, the docs pages would miss the rules gathered from other entries.
        cssCodeSplit: false,
        sourcemap: sentryAuthToken ? "hidden" : false,
        rollupOptions: { input: pageInputs },
      },
}));
