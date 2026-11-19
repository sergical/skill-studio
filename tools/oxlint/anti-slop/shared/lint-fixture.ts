// Runs Oxlint with only the named anti-slop rules on one source string, for the
// integration tests next to this file.

import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { z } from "zod";

interface OxlintJsonDiagnostic {
	readonly code: string;
	readonly message: string;
}

const oxlintJsonOutputSchema = z.object({
	diagnostics: z.array(z.object({ code: z.string(), message: z.string() })),
});

/** Lints `source`, saved as `fileName`, with only `rules` on; returns the anti-slop diagnostics. */
export function lintAntiSlopFixture(
	source: string,
	rules: readonly string[],
	fileName = "fixture.ts",
): readonly OxlintJsonDiagnostic[] {
	const fixtureDirectory = mkdtempSync(join(tmpdir(), "anti-slop-fixture-"));
	const fixturePath = join(fixtureDirectory, fileName);
	const configPath = join(fixtureDirectory, "oxlint.json");
	writeFileSync(fixturePath, source);
	writeFileSync(
		configPath,
		JSON.stringify({
			categories: { correctness: "off" },
			jsPlugins: [
				{
					name: "anti-slop",
					specifier: resolve("tools/oxlint/anti-slop/index.ts"),
				},
			],
			rules: Object.fromEntries(rules.map((rule) => [`anti-slop/${rule}`, "error"])),
		}),
	);

	try {
		const result = spawnSync(
			resolve("node_modules/.bin/oxlint"),
			["--config", configPath, "--format", "json", fixturePath],
			{ encoding: "utf8", timeout: 10_000 },
		);
		if (result.error !== undefined) throw result.error;
		if (result.status !== 0 && result.status !== 1) {
			throw new Error(`Anti-slop Oxlint fixture failed: ${result.stderr || result.stdout}`);
		}
		return oxlintJsonOutputSchema
			.parse(JSON.parse(result.stdout))
			.diagnostics.filter((diagnostic) => diagnostic.code.startsWith("anti-slop("));
	} finally {
		rmSync(fixtureDirectory, { recursive: true, force: true });
	}
}

export function diagnosticCount(diagnostics: readonly OxlintJsonDiagnostic[], code: string): number {
	return diagnostics.filter((diagnostic) => diagnostic.code === `anti-slop(${code})`).length;
}
