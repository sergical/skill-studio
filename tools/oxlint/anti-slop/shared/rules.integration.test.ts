import { describe, expect, it } from "vitest";

import { diagnosticCount, lintAntiSlopFixture } from "./lint-fixture.ts";

describe("anti-slop no-raw-form-elements", () => {
	it("flags each raw form control and allows the @skill-studio/ui primitives", () => {
		const diagnostics = lintAntiSlopFixture(
			`import { Button, Input } from "@skill-studio/ui";
			export function Form() {
				return (
					<form>
						<input />
						<textarea />
						<select />
						<button type="button">Raw</button>
						<Input />
						<Button>Primitive</Button>
						<div>not a control</div>
					</form>
				);
			}`,
			["no-raw-form-elements"],
			"fixture.tsx",
		);

		expect(diagnosticCount(diagnostics, "no-raw-form-elements")).toBe(4);
		expect(diagnostics.map((diagnostic) => diagnostic.message)).toEqual(
			expect.arrayContaining([expect.stringContaining("Input primitive")]),
		);
	});
});

describe("anti-slop no-module-mocking", () => {
	it("flags vi and jest module mocks and allows spies and fake timers", () => {
		const diagnostics = lintAntiSlopFixture(
			`import { vi } from "vitest";
			import { jest } from "@jest/globals";
			const api = { load: () => 1 };
			vi.mock("./skill-api");
			vi.doMock("./skill-api");
			jest.mock("./skill-api");
			jest.unstable_mockModule("./skill-api", () => ({}));
			vi.spyOn(api, "load");
			vi.useFakeTimers();
			vi.fn();`,
			["no-module-mocking"],
		);

		expect(diagnosticCount(diagnostics, "no-module-mocking")).toBe(4);
	});
});
