export const SITE_URL = "https://useskillstudio.com/";
export const GITHUB_REPOSITORY_URL = "https://github.com/sergical/skill-studio";
// Branded on-site download routes; Cloudflare _redirects (and Vite dev middleware) 302 these to
// the latest release DMGs so buttons never expose raw GitHub release URLs.
export const DOWNLOAD_URL = "/download";
export const DOWNLOAD_INTEL_URL = "/download/intel";
export const DOWNLOAD_REDIRECTS = new Map<string, string>([
  [DOWNLOAD_URL, `${GITHUB_REPOSITORY_URL}/releases/latest/download/Skill-Studio-arm64.dmg`],
  [DOWNLOAD_INTEL_URL, `${GITHUB_REPOSITORY_URL}/releases/latest/download/Skill-Studio-intel.dmg`],
]);
export const DOCS_URL = "/docs/";
export const CLI_DOCS_URL = "/docs/cli/";
export const MCP_DOCS_URL = "/docs/mcp/";

// Keep in step with bundle.macOS.minimumSystemVersion in apps/desktop/src-tauri/tauri.conf.json.
const REQUIREMENTS = "macOS 13+";
export const TRUST_LINE = `Free and open source · ${REQUIREMENTS}`;
