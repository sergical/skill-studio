import { describe, expect, it } from "vitest";
import tauriConfig from "../../src-tauri/tauri.conf.json";

const { csp } = tauriConfig.app.security;

describe("release CSP", () => {
  it("runs only the app's own scripts and talks only to Tauri IPC", () => {
    expect(csp["script-src"]).toBe("'self'");
    expect(csp["connect-src"]).toBe("ipc: http://ipc.localhost");
    expect(csp["object-src"]).toBe("'none'");
  });
});
