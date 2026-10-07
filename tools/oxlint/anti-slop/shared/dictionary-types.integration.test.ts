import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { describe, expect, it } from "vitest";
import { z } from "zod";

import { createTypeEnvironment } from "./dictionary-types.ts";

import type { ESTree } from "@oxlint/plugins";

interface OxlintJsonDiagnostic {
	readonly code: string;
	readonly message: string;
}

const oxlintJsonOutputSchema = z.object({
	diagnostics: z.array(z.object({ code: z.string(), message: z.string() })),
});

function parseOxlintDiagnostics(stdout: string): readonly OxlintJsonDiagnostic[] {
	return oxlintJsonOutputSchema.parse(JSON.parse(stdout)).diagnostics;
}

function lintAntiSlopFixture(
	source: string,
	rules: readonly string[],
): readonly OxlintJsonDiagnostic[] {
	const fixtureDirectory = mkdtempSync(join(tmpdir(), "anti-slop-dictionary-types-"));
	const fixturePath = join(fixtureDirectory, "fixture.ts");
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
		return parseOxlintDiagnostics(result.stdout).filter((diagnostic) =>
			diagnostic.code.startsWith("anti-slop("),
		);
	} finally {
		rmSync(fixtureDirectory, { recursive: true, force: true });
	}
}

function diagnosticCount(diagnostics: readonly OxlintJsonDiagnostic[], code: string): number {
	return diagnostics.filter((diagnostic) => diagnostic.code === `anti-slop(${code})`).length;
}

describe("anti-slop interface dictionary rules", () => {
	it("binds unknown, any, and default generic interface arguments", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface GenericDictionary<Value> {
				[key: string]: Value;
			}
			interface DefaultDictionary<Value = unknown> {
				[key: string]: Value;
			}
			const unknownDictionary: GenericDictionary<unknown> = {};
			const anyDictionary: GenericDictionary<any> = {};
			const defaultDictionary: DefaultDictionary = {};
			const safeDictionary: GenericDictionary<string> = {};
			void unknownDictionary;
			void anyDictionary;
			void defaultDictionary;
			void safeDictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(3);
		expect(diagnostics.filter((diagnostic) => diagnostic.message.includes("unknown"))).toHaveLength(
			2,
		);
		expect(diagnostics.filter((diagnostic) => diagnostic.message.includes("any"))).toHaveLength(1);
	});

	it("flags known evidence assigned and asserted to a generic interface dictionary", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface GenericDictionary<Value> {
				[key: string]: Value;
			}
			interface OpenDictionary {
				[key: string]: string;
			}
			const assigned: GenericDictionary<unknown> = { known: "value" };
			const asserted = { known: "value" } as GenericDictionary<unknown>;
			const openDictionary: OpenDictionary = { known: "value" };
			void assigned;
			void asserted;
			void openDictionary;`,
			["no-known-value-widening"],
		);

		expect(diagnosticCount(diagnostics, "no-known-value-widening")).toBe(3);
		expect(
			diagnostics.filter((diagnostic) => diagnostic.message.includes("generic container")),
		).toHaveLength(2);
		expect(
			diagnostics.filter((diagnostic) => diagnostic.message.includes("open dictionary")),
		).toHaveLength(1);
	});

	it("classifies generic and plain alias wrappers around interface dictionaries", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface GenericDictionary<Value> {
				[key: string]: Value;
			}
			type GenericWrapper<Value> = GenericDictionary<Value>;
			type PlainWrapper = GenericDictionary<string>;
			const genericWrapper: GenericWrapper<unknown> = { known: "value" };
			const plainWrapper: PlainWrapper = { known: "value" };
			void genericWrapper;
			void plainWrapper;`,
			["no-unsafe-dictionary-type", "no-known-value-widening"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(1);
		expect(diagnosticCount(diagnostics, "no-known-value-widening")).toBe(2);
		expect(
			diagnostics.filter((diagnostic) => diagnostic.message.includes("generic container")),
		).toHaveLength(1);
		expect(
			diagnostics.filter((diagnostic) => diagnostic.message.includes("open dictionary")),
		).toHaveLength(1);
	});

	it("collects substituted inherited and merged interface index signatures", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface BaseDictionary<Value> {
				[key: string]: Value;
			}
			interface InheritedDictionary<Payload> extends BaseDictionary<Payload> {}
			interface MergedDictionary<Value> {
				label?: string;
			}
			interface MergedDictionary<Value> {
				[key: string]: Value;
			}
			const inheritedUnsafe: InheritedDictionary<unknown> = {};
			const inheritedSafe: InheritedDictionary<string> = {};
			const mergedUnsafe: MergedDictionary<unknown> = {};
			void inheritedUnsafe;
			void inheritedSafe;
			void mergedUnsafe;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(2);
	});

	it("binds explicit heritage arguments in the caller scope", () => {
		const unsafeDiagnostics = lintAntiSlopFixture(
			`interface Base<A, B> {
				[key: string]: B;
			}
			interface Derived<A, B> extends Base<B, A> {}
			const dictionary: Derived<unknown, string> = {};
			void dictionary;`,
			["no-unsafe-dictionary-type"],
		);
		const safeDiagnostics = lintAntiSlopFixture(
			`interface Base<A, B> {
				[key: string]: B;
			}
			interface Derived<A, B> extends Base<B, A> {}
			const dictionary: Derived<string, unknown> = {};
			void dictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(unsafeDiagnostics, "no-unsafe-dictionary-type")).toBe(1);
		expect(safeDiagnostics).toEqual([]);
	});

	it("keeps a caller Readonly parameter out of the base interface scope", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<Value> {
				[key: string]: Readonly<Value>;
			}
			interface Derived<Readonly> extends Base<string> {}
			const dictionary: Derived<unknown> = {};
			void dictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnostics).toEqual([]);
	});

	it("keeps a caller parameter out of the base interface global-alias scope", () => {
		const diagnostics = lintAntiSlopFixture(
			`type GlobalValue = string;
			interface Base {
				[key: string]: GlobalValue;
			}
			interface Derived<GlobalValue> extends Base {}
			const dictionary: Derived<unknown> = {};
			void dictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnostics).toEqual([]);
	});

	it("preserves caller substitutions inside nested heritage arguments", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<Value> {
				[key: string]: Value;
			}
			interface Derived<Value> extends Base<Readonly<Value>> {}
			const dictionary: Derived<unknown> = {};
			void dictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(1);
		expect(diagnostics[0]?.message).toContain("unknown");
	});

	it("resolves same-named caller arguments through their saved scopes", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = unknown;
			type AliasDictionary<Value> = Record<string, Value>;
			interface InterfaceDictionary<Value> extends Record<string, Value> {}
			const aliasDictionary: AliasDictionary<Value> = {};
			const interfaceDictionary: InterfaceDictionary<Value> = {};
			void aliasDictionary;
			void interfaceDictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(2);
		expect(diagnostics.every((diagnostic) => diagnostic.message.includes("unknown"))).toBe(true);
	});

	it("keeps generic function parameters unresolved in nested lexical scopes", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = unknown;
			type OuterValue = unknown;
			interface Dictionary<Entry> {
				[key: string]: Entry;
			}
			function consumeDeclaration<Value>(input: Dictionary<Value>) {
				void input;
			}
			const consumeExpression = function <Value>(input: Dictionary<Value>) {
				void input;
			};
			const consumeArrow = <Value>(input: Dictionary<Value>) => void input;
			declare function consumeDeclared<Value>(input: Dictionary<Value>): void;
			function consumeOverload<Value>(input: Dictionary<Value>): void;
			function consumeOverload(input: Dictionary<string>): void {
				void input;
			}
			function consumeOuter<OuterValue>() {
				return <InnerValue>(input: Dictionary<OuterValue>) => void input;
			}
			function consumeBuiltIn<Record>(input: Dictionary<Record>) {
				void input;
			}
			void consumeDeclaration;
			void consumeExpression;
			void consumeArrow;
			void consumeDeclared;
			void consumeOverload;
			void consumeOuter;
			void consumeBuiltIn;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnostics).toEqual([]);
	});

	it("keeps generic class parameters unresolved in class declarations and expressions", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = unknown;
			type MethodValue = unknown;
			interface Dictionary<Entry> {
				[key: string]: Entry;
			}
			class DictionaryOwner<Value> {
				declare entries: Dictionary<Value>;
				consume(input: Dictionary<Value>) {
					void input;
				}
				consumeGeneric<MethodValue>(input: Dictionary<MethodValue>) {
					void input;
				}
			}
			const DictionaryOwnerExpression = class<Value> {
				declare entries: Dictionary<Value>;
			};
			void DictionaryOwner;
			void DictionaryOwnerExpression;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnostics).toEqual([]);
	});

	it("keeps signature type parameters unresolved on their owning AST nodes", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = unknown;
			interface Dictionary<Entry> {
				[key: string]: Entry;
			}
			interface GenericSignatures {
				<Value>(input: Dictionary<Value>): void;
				new <Value>(input: Dictionary<Value>): object;
				consume<Value>(input: Dictionary<Value>): void;
			}
			type GenericFunction = <Value>(input: Dictionary<Value>) => void;
			type GenericConstructor = new <Value>(input: Dictionary<Value>) => object;
			abstract class AbstractConsumer {
				abstract consume<Value>(input: Dictionary<Value>): void;
			}
			let signatures: GenericSignatures;
			let genericFunction: GenericFunction;
			let genericConstructor: GenericConstructor;
			void signatures;
			void genericFunction;
			void genericConstructor;
			void AbstractConsumer;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnostics).toEqual([]);
	});

	it("reports concrete alias instantiations outside generic lexical scopes", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = unknown;
			interface Dictionary<Entry> {
				[key: string]: Entry;
			}
			function consume<Value>(input: Dictionary<Value>) {
				void input;
			}
			const concreteDictionary: Dictionary<Value> = {};
			void consume;
			void concreteDictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(1);
		expect(diagnostics[0]?.message).toContain("unknown");
	});

	it("keeps a caller PropertyKey parameter out of mapped alias widening scope", () => {
		const diagnostics = lintAntiSlopFixture(
			`type BaseDictionary = { [Key in PropertyKey]: string };
			type DerivedDictionary<PropertyKey> = BaseDictionary;
			type ConcreteDictionary = DerivedDictionary<"known">;
			const dictionary: ConcreteDictionary = { known: "value" };
			void dictionary;`,
			["no-known-value-widening"],
		);

		expect(diagnosticCount(diagnostics, "no-known-value-widening")).toBe(1);
		expect(diagnostics[0]?.message).toContain("open dictionary");
	});

	it("keeps a mapped key parameter out of its value's global alias scope", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Key = unknown;
			type SafeMappedDictionary = { [Key in "known"]: Key };
			const dictionary: SafeMappedDictionary = { known: "known" };
			void dictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnostics).toEqual([]);
	});

	it("binds mapped keys only in value and remapped-name branches", () => {
		const scopedDiagnostics = lintAntiSlopFixture(
			`type Value = unknown;
			interface Dictionary<Entry> {
				[key: string]: Entry;
			}
			type MappedDictionary = { [Value in "known"]: Dictionary<Value> };
			type RemappedDictionary = {
				[Value in "known" as keyof Dictionary<Value>]: string;
			};
			const mappedDictionary: MappedDictionary = { known: {} };
			const remappedDictionary: RemappedDictionary = { known: "known" };
			void mappedDictionary;
			void remappedDictionary;`,
			["no-unsafe-dictionary-type"],
		);
		const unscopedDiagnostics = lintAntiSlopFixture(
			`type Value = unknown;
			interface Dictionary<Entry> {
				[key: string]: Entry;
			}
			type MappedConstraint = { [Value in keyof Dictionary<Value>]: string };
			const concreteDictionary: Dictionary<Value> = {};
			void concreteDictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(scopedDiagnostics).toEqual([]);
		expect(diagnosticCount(unscopedDiagnostics, "no-unsafe-dictionary-type")).toBe(2);
		expect(
			unscopedDiagnostics.every((diagnostic) => diagnostic.message.includes("unknown")),
		).toBe(true);
	});

	it("limits conditional infer parameters to the true branch", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = unknown;
			interface Dictionary<Entry> {
				[key: string]: Entry;
			}
			type Direct<T> = T extends infer Value ? Dictionary<Value> : never;
			type Nested<T> = T extends Promise<infer Value>
				? Dictionary<Value>
				: Dictionary<Value>;
			const concreteDictionary: Dictionary<Value> = {};
			void concreteDictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(2);
		expect(diagnostics.every((diagnostic) => diagnostic.message.includes("unknown"))).toBe(true);
	});

	it("applies merged interface defaults to every declaration", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Dictionary<Value> {
				[key: string]: Value;
			}
			interface Dictionary<Value = unknown> {}
			const defaultDictionary: Dictionary = {};
			const explicitUnsafeDictionary: Dictionary<any> = {};
			const explicitSafeDictionary: Dictionary<string> = {};
			void defaultDictionary;
			void explicitUnsafeDictionary;
			void explicitSafeDictionary;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(2);
		expect(diagnostics.filter((diagnostic) => diagnostic.message.includes("unknown"))).toHaveLength(
			1,
		);
		expect(diagnostics.filter((diagnostic) => diagnostic.message.includes("any"))).toHaveLength(1);
	});

	it("resolves interface heritage through aliases and transparent wrappers", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Base<Value> = { [key: string]: Value };
			interface AliasDictionary<Value> extends Base<Value> {}
			interface ReadonlyDictionary<Value> extends Readonly<Base<Value>> {}
			const aliasUnsafe: AliasDictionary<unknown> = {};
			const aliasSafe: AliasDictionary<string> = {};
			const readonlyUnsafe: ReadonlyDictionary<any> = {};
			const readonlySafe: ReadonlyDictionary<string> = {};
			void aliasUnsafe;
			void aliasSafe;
			void readonlyUnsafe;
			void readonlySafe;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(2);
		expect(diagnostics.filter((diagnostic) => diagnostic.message.includes("unknown"))).toHaveLength(
			1,
		);
		expect(diagnostics.filter((diagnostic) => diagnostic.message.includes("any"))).toHaveLength(1);
	});

	it("resolves wrapper-named type parameters before built-in wrappers", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface ReadonlyDictionary<Readonly> {
				[key: string]: Readonly;
			}
			interface PartialDictionary<Partial> {
				[key: string]: Partial;
			}
			interface RequiredDictionary<Required> {
				[key: string]: Required;
			}
			interface NonNullableDictionary<NonNullable> {
				[key: string]: NonNullable;
			}
			interface ValueDictionary<Value> {
				[key: string]: Value;
			}
			const readonlyParameter: ReadonlyDictionary<unknown> = {};
			const partialParameter: PartialDictionary<unknown> = {};
			const requiredParameter: RequiredDictionary<unknown> = {};
			const nonNullableParameter: NonNullableDictionary<unknown> = {};
			const builtInUnsafe: ValueDictionary<Readonly<unknown>> = {};
			const builtInSafe: ValueDictionary<Readonly<string>> = {};
			void readonlyParameter;
			void partialParameter;
			void requiredParameter;
			void nonNullableParameter;
			void builtInUnsafe;
			void builtInSafe;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(5);
		expect(diagnostics.every((diagnostic) => diagnostic.message.includes("unknown"))).toBe(true);
	});

	it("terminates mixed alias and interface heritage cycles", () => {
		const diagnostics = lintAntiSlopFixture(
			`type AliasCycle<Value> = InterfaceCycle<Value>;
			type Base<Value> = { [key: string]: Value };
			interface InterfaceCycle<Value> extends AliasCycle<Value>, Base<Value> {}
			const unsafeCycle: InterfaceCycle<unknown> = {};
			const safeCycle: InterfaceCycle<string> = {};
			void unsafeCycle;
			void safeCycle;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(1);
	});

	it("reports one concrete interface definition once for two consumers", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface DirectUnsafeDictionary {
				[key: string]: unknown;
			}
			const firstDirectConsumer: DirectUnsafeDictionary = {};
			const secondDirectConsumer: DirectUnsafeDictionary = {};
			void firstDirectConsumer;
			void secondDirectConsumer;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(1);
		expect(diagnostics[0]?.message).toContain("unknown");
	});

	it("keeps generic, defaulted, and inherited-substitution consumers reportable", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface GenericDictionary<Value> {
				[key: string]: Value;
			}
			interface InheritedUnsafeDictionary extends GenericDictionary<unknown> {}
			interface DefaultDictionary<Value> {
				[key: string]: Value;
			}
			interface DefaultDictionary<Value = unknown> {}
			const genericConsumer: GenericDictionary<unknown> = {};
			const inheritedConsumer: InheritedUnsafeDictionary = {};
			const defaultConsumer: DefaultDictionary = {};
			void genericConsumer;
			void inheritedConsumer;
			void defaultConsumer;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(3);
		expect(diagnostics.every((diagnostic) => diagnostic.message.includes("unknown"))).toBe(true);
	});

	it("keeps safe, non-dictionary, and cyclic interfaces opaque", () => {
		const safeDiagnostics = lintAntiSlopFixture(
			`interface SafeDictionary {
				[key: string]: string;
			}
			const safeDictionary: SafeDictionary = {};
			void safeDictionary;`,
			["no-unsafe-dictionary-type"],
		);
		const opaqueDiagnostics = lintAntiSlopFixture(
			`interface GenericObject<Value> {
				value: Value;
			}
			interface SelfCycle<Value> extends SelfCycle<Value> {}
			interface LeftCycle<Value> extends RightCycle<Value> {}
			interface RightCycle<Value> extends LeftCycle<Value> {}
			const objectValue: GenericObject<unknown> = { value: "known" };
			const selfCycle: SelfCycle<unknown> = { known: "value" };
			const mutualCycle = { known: "value" } as LeftCycle<unknown>;
			void objectValue;
			void selfCycle;
			void mutualCycle;`,
			["no-unsafe-dictionary-type", "no-known-value-widening"],
		);

		expect(safeDiagnostics).toEqual([]);
		expect(opaqueDiagnostics).toEqual([]);
	});

	it("preserves alias, mapped, and empty-interface dictionary handling", () => {
		const diagnostics = lintAntiSlopFixture(
			`type AliasDictionary<Value> = { [key: string]: Value };
			type MappedDictionary<Key extends string, Value> = { [Property in Key]: Value };
			interface EmptyValue {}
			type EmptyValueDictionary = { [key: string]: EmptyValue };
			const aliasDictionary: AliasDictionary<unknown> = { known: "value" };
			const mappedDictionary: MappedDictionary<string, unknown> = { known: "value" };
			const emptyValueDictionary: EmptyValueDictionary = { known: {} };
			void aliasDictionary;
			void mappedDictionary;
			void emptyValueDictionary;`,
			["no-unsafe-dictionary-type", "no-known-value-widening"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(3);
		expect(diagnosticCount(diagnostics, "no-known-value-widening")).toBe(3);
		expect(
			diagnostics.filter((diagnostic) => diagnostic.message.includes("generic container")),
		).toHaveLength(2);
		expect(
			diagnostics.filter((diagnostic) => diagnostic.message.includes("open dictionary")),
		).toHaveLength(1);
	});

	it("preserves built-in and shadowed Record handling", () => {
		const builtInDiagnostics = lintAntiSlopFixture(
			`const dictionary: Record<string, unknown> = { known: "value" };
			void dictionary;`,
			["no-unsafe-dictionary-type", "no-known-value-widening"],
		);
		const shadowedDiagnostics = lintAntiSlopFixture(
			`interface Record<Key, Value> {
				key: Key;
				value: Value;
			}
			const record: Record<string, unknown> = { key: "known", value: "known" };
			void record;`,
			["no-unsafe-dictionary-type", "no-known-value-widening"],
		);

		expect(diagnosticCount(builtInDiagnostics, "no-unsafe-dictionary-type")).toBe(1);
		expect(diagnosticCount(builtInDiagnostics, "no-known-value-widening")).toBe(1);
		expect(
			builtInDiagnostics.some((diagnostic) => diagnostic.message.includes("open dictionary")),
		).toBe(true);
		expect(shadowedDiagnostics).toEqual([]);
	});

	it("drops a non-generic inherited unsafe index signature narrowed by a same-key-kind override", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base {
				[key: string]: unknown;
			}
			interface Derived extends Base {
				[key: string]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(1);
		expect(diagnostics[0]?.message).toContain("unknown");
	});

	it("drops a generic inherited unsafe index signature narrowed by a same-key-kind override", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<Value> {
				[key: string]: Value;
			}
			interface Derived extends Base<unknown> {
				[key: string]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnostics).toEqual([]);
	});

	it("keeps reporting a derived consumer when its body declares no override of an inherited unsafe signature", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base {
				[key: string]: unknown;
			}
			interface Derived extends Base {}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(2);
		expect(diagnostics.every((diagnostic) => diagnostic.message.includes("unknown"))).toBe(true);
	});

	it("still reports a partial override that leaves an inherited unsafe key kind in place", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base {
				[key: string]: unknown;
				[key: number]: unknown;
			}
			interface Derived extends Base {
				[key: string]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(3);
		expect(diagnostics.every((diagnostic) => diagnostic.message.includes("unknown"))).toBe(true);
	});

	it("keeps the inherited unsafe signature when the direct override covers a different key kind", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base {
				[key: string]: unknown;
			}
			interface Derived extends Base {
				[key: number]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(2);
		expect(diagnostics.every((diagnostic) => diagnostic.message.includes("unknown"))).toBe(true);
	});

	it("drops an inherited unsafe union value narrowed to a safe member by an override", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base {
				[key: string]: unknown | string;
			}
			interface Derived extends Base {
				[key: string]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(1);
		expect(diagnostics[0]?.message).toContain("union");
	});

	it("accepts an inherited unknown dictionary when the override value is a concrete object literal, not flagged as unsafe", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: { id: string };
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("accepts an inherited unknown dictionary when the override value is an array of strings, not flagged as unsafe", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: string[] | readonly string[] | [string, number];
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("accepts an inherited unknown dictionary when the override value is a local string alias, not flagged as unsafe", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = string;
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: Value;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("keeps an inherited unknown dictionary flagged when the override value is a cyclic local alias", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Loop = Loop[] | string;
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: Loop;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("keeps an inherited unknown dictionary flagged when the override value is an unresolved imported alias", () => {
		const diagnostics = lintAntiSlopFixture(
			`import type { Imported } from "./other";
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: Imported;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("keeps an inherited unknown dictionary flagged, without throwing, when the override object literal has an untyped property", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: { id; };
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("classifies an override through a 40-alias doubling chain without re-reading shared aliases, so the fixture finishes inside the Oxlint timeout", () => {
		const diagnostics = lintAntiSlopFixture(
			`type A0 = string;
			type A1 = [A0, A0];
			type A2 = [A1, A1];
			type A3 = [A2, A2];
			type A4 = [A3, A3];
			type A5 = [A4, A4];
			type A6 = [A5, A5];
			type A7 = [A6, A6];
			type A8 = [A7, A7];
			type A9 = [A8, A8];
			type A10 = [A9, A9];
			type A11 = [A10, A10];
			type A12 = [A11, A11];
			type A13 = [A12, A12];
			type A14 = [A13, A13];
			type A15 = [A14, A14];
			type A16 = [A15, A15];
			type A17 = [A16, A16];
			type A18 = [A17, A17];
			type A19 = [A18, A18];
			type A20 = [A19, A19];
			type A21 = [A20, A20];
			type A22 = [A21, A21];
			type A23 = [A22, A22];
			type A24 = [A23, A23];
			type A25 = [A24, A24];
			type A26 = [A25, A25];
			type A27 = [A26, A26];
			type A28 = [A27, A27];
			type A29 = [A28, A28];
			type A30 = [A29, A29];
			type A31 = [A30, A30];
			type A32 = [A31, A31];
			type A33 = [A32, A32];
			type A34 = [A33, A33];
			type A35 = [A34, A34];
			type A36 = [A35, A35];
			type A37 = [A36, A36];
			type A38 = [A37, A37];
			type A39 = [A38, A38];
			type A40 = [A39, A39];
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: A40;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("accepts a safe override declared after an inheriting declaration of the same merged interface, not flagged as unsafe", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {}
			interface Derived {
				[key: string]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("accepts a safe override declared before an inheriting declaration of the same merged interface, not flagged as unsafe", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived {
				[key: string]: string;
			}
			interface Derived extends Base<unknown> {}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("keeps an inherited unknown dictionary flagged when the override value is a named Array<string>, which a declaration could shadow", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: Array<string>;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("keeps an inherited unknown dictionary flagged when Array is an import-equals alias of an unsafe type", () => {
		const diagnostics = lintAntiSlopFixture(
			`namespace N {
				export type Unsafe = unknown;
			}
			import Array = N.Unsafe;
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: Array<string>;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("classifies an override through a 40-default doubling chain of generic defaults without re-reading shared defaults, so the fixture finishes inside the Oxlint timeout", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived<A0 = string, A1 = [A0, A0], A2 = [A1, A1], A3 = [A2, A2], A4 = [A3, A3], A5 = [A4, A4], A6 = [A5, A5], A7 = [A6, A6], A8 = [A7, A7], A9 = [A8, A8], A10 = [A9, A9], A11 = [A10, A10], A12 = [A11, A11], A13 = [A12, A12], A14 = [A13, A13], A15 = [A14, A14], A16 = [A15, A15], A17 = [A16, A16], A18 = [A17, A17], A19 = [A18, A18], A20 = [A19, A19], A21 = [A20, A20], A22 = [A21, A21], A23 = [A22, A22], A24 = [A23, A23], A25 = [A24, A24], A26 = [A25, A25], A27 = [A26, A26], A28 = [A27, A27], A29 = [A28, A28], A30 = [A29, A29], A31 = [A30, A30], A32 = [A31, A31], A33 = [A32, A32], A34 = [A33, A33], A35 = [A34, A34], A36 = [A35, A35], A37 = [A36, A36], A38 = [A37, A37], A39 = [A38, A38], A40 = [A39, A39]> extends Base<unknown> {
				[key: string]: A40;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("keeps an inherited unknown dictionary flagged when a nested type alias shadows the top-level alias used as the override argument", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = string;
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived<T> extends Base<unknown> {
				[key: string]: T;
			}
			function run() {
				type Value = unknown;
				const d: Derived<Value> = { entry: 42 };
				void d;
			}`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("accepts an inherited unknown dictionary when the top-level alias used as the override argument is not shadowed, not flagged as unsafe", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = string;
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived<T> extends Base<unknown> {
				[key: string]: T;
			}
			function run() {
				const d: Derived<Value> = { entry: "x" };
				void d;
			}`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("keeps an inherited unknown dictionary flagged when a nested interface reuses the name of a top-level interface with a string override", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: string;
			}
			function run() {
				interface Derived extends Base<unknown> {}
				const d: Derived = { entry: 42 };
				void d;
			}`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("accepts a top-level interface with a string override when no nested interface reuses its name, not flagged as unsafe", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: string;
			}
			const d: Derived = { entry: "x" };
			void d;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("keeps an inherited unknown dictionary flagged when a nested interface reuses the name inside a top-level arrow function variable", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: string;
			}
			const run = () => {
				interface Derived extends Base<unknown> {}
				const d: Derived = { entry: 42 };
				void d;
			};
			void run;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("keeps an inherited unknown dictionary flagged when a nested alias reuses the name inside a top-level arrow function variable", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Value = string;
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived<T> extends Base<unknown> {
				[key: string]: T;
			}
			const run = () => {
				type Value = unknown;
				const d: Derived<Value> = { entry: 42 };
				void d;
			};
			void run;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("keeps an inherited unknown dictionary flagged when a nested interface reuses the name inside a top-level block", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: string;
			}
			{
				interface Derived extends Base<unknown> {}
				const d: Derived = { entry: 42 };
				void d;
			}`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("records nested type names of a 12,000-term sum without overflowing the stack", () => {
		interface SyntheticNode {
			readonly type: string;
			readonly left?: SyntheticNode;
			readonly right?: SyntheticNode;
		}
		// Oxlint's own deserializer overflows near 2,700 terms, so build the tree directly.
		let sum: SyntheticNode = { type: "Literal" };
		for (let term = 1; term < 12_000; term += 1) {
			sum = { type: "BinaryExpression", left: sum, right: { type: "Literal" } };
		}
		const body: readonly SyntheticNode[] = [{ type: "ExpressionStatement", left: sum }];
		// SAFETY: createTypeEnvironment only reads `type` and child nodes of each statement.
		const program = { type: "Program", body } as ESTree.Program;

		expect(createTypeEnvironment(program).nestedTypeNames.size).toBe(0);
	});

	it("keeps an inherited unknown dictionary flagged, without overflowing the stack, when the override alias chain is 2,500 deep", () => {
		const diagnostics = lintAntiSlopFixture(
			`type A0 = string;
			type A1 = A0[];
			type A2 = A1[];
			type A3 = A2[];
			type A4 = A3[];
			type A5 = A4[];
			type A6 = A5[];
			type A7 = A6[];
			type A8 = A7[];
			type A9 = A8[];
			type A10 = A9[];
			type A11 = A10[];
			type A12 = A11[];
			type A13 = A12[];
			type A14 = A13[];
			type A15 = A14[];
			type A16 = A15[];
			type A17 = A16[];
			type A18 = A17[];
			type A19 = A18[];
			type A20 = A19[];
			type A21 = A20[];
			type A22 = A21[];
			type A23 = A22[];
			type A24 = A23[];
			type A25 = A24[];
			type A26 = A25[];
			type A27 = A26[];
			type A28 = A27[];
			type A29 = A28[];
			type A30 = A29[];
			type A31 = A30[];
			type A32 = A31[];
			type A33 = A32[];
			type A34 = A33[];
			type A35 = A34[];
			type A36 = A35[];
			type A37 = A36[];
			type A38 = A37[];
			type A39 = A38[];
			type A40 = A39[];
			type A41 = A40[];
			type A42 = A41[];
			type A43 = A42[];
			type A44 = A43[];
			type A45 = A44[];
			type A46 = A45[];
			type A47 = A46[];
			type A48 = A47[];
			type A49 = A48[];
			type A50 = A49[];
			type A51 = A50[];
			type A52 = A51[];
			type A53 = A52[];
			type A54 = A53[];
			type A55 = A54[];
			type A56 = A55[];
			type A57 = A56[];
			type A58 = A57[];
			type A59 = A58[];
			type A60 = A59[];
			type A61 = A60[];
			type A62 = A61[];
			type A63 = A62[];
			type A64 = A63[];
			type A65 = A64[];
			type A66 = A65[];
			type A67 = A66[];
			type A68 = A67[];
			type A69 = A68[];
			type A70 = A69[];
			type A71 = A70[];
			type A72 = A71[];
			type A73 = A72[];
			type A74 = A73[];
			type A75 = A74[];
			type A76 = A75[];
			type A77 = A76[];
			type A78 = A77[];
			type A79 = A78[];
			type A80 = A79[];
			type A81 = A80[];
			type A82 = A81[];
			type A83 = A82[];
			type A84 = A83[];
			type A85 = A84[];
			type A86 = A85[];
			type A87 = A86[];
			type A88 = A87[];
			type A89 = A88[];
			type A90 = A89[];
			type A91 = A90[];
			type A92 = A91[];
			type A93 = A92[];
			type A94 = A93[];
			type A95 = A94[];
			type A96 = A95[];
			type A97 = A96[];
			type A98 = A97[];
			type A99 = A98[];
			type A100 = A99[];
			type A101 = A100[];
			type A102 = A101[];
			type A103 = A102[];
			type A104 = A103[];
			type A105 = A104[];
			type A106 = A105[];
			type A107 = A106[];
			type A108 = A107[];
			type A109 = A108[];
			type A110 = A109[];
			type A111 = A110[];
			type A112 = A111[];
			type A113 = A112[];
			type A114 = A113[];
			type A115 = A114[];
			type A116 = A115[];
			type A117 = A116[];
			type A118 = A117[];
			type A119 = A118[];
			type A120 = A119[];
			type A121 = A120[];
			type A122 = A121[];
			type A123 = A122[];
			type A124 = A123[];
			type A125 = A124[];
			type A126 = A125[];
			type A127 = A126[];
			type A128 = A127[];
			type A129 = A128[];
			type A130 = A129[];
			type A131 = A130[];
			type A132 = A131[];
			type A133 = A132[];
			type A134 = A133[];
			type A135 = A134[];
			type A136 = A135[];
			type A137 = A136[];
			type A138 = A137[];
			type A139 = A138[];
			type A140 = A139[];
			type A141 = A140[];
			type A142 = A141[];
			type A143 = A142[];
			type A144 = A143[];
			type A145 = A144[];
			type A146 = A145[];
			type A147 = A146[];
			type A148 = A147[];
			type A149 = A148[];
			type A150 = A149[];
			type A151 = A150[];
			type A152 = A151[];
			type A153 = A152[];
			type A154 = A153[];
			type A155 = A154[];
			type A156 = A155[];
			type A157 = A156[];
			type A158 = A157[];
			type A159 = A158[];
			type A160 = A159[];
			type A161 = A160[];
			type A162 = A161[];
			type A163 = A162[];
			type A164 = A163[];
			type A165 = A164[];
			type A166 = A165[];
			type A167 = A166[];
			type A168 = A167[];
			type A169 = A168[];
			type A170 = A169[];
			type A171 = A170[];
			type A172 = A171[];
			type A173 = A172[];
			type A174 = A173[];
			type A175 = A174[];
			type A176 = A175[];
			type A177 = A176[];
			type A178 = A177[];
			type A179 = A178[];
			type A180 = A179[];
			type A181 = A180[];
			type A182 = A181[];
			type A183 = A182[];
			type A184 = A183[];
			type A185 = A184[];
			type A186 = A185[];
			type A187 = A186[];
			type A188 = A187[];
			type A189 = A188[];
			type A190 = A189[];
			type A191 = A190[];
			type A192 = A191[];
			type A193 = A192[];
			type A194 = A193[];
			type A195 = A194[];
			type A196 = A195[];
			type A197 = A196[];
			type A198 = A197[];
			type A199 = A198[];
			type A200 = A199[];
			type A201 = A200[];
			type A202 = A201[];
			type A203 = A202[];
			type A204 = A203[];
			type A205 = A204[];
			type A206 = A205[];
			type A207 = A206[];
			type A208 = A207[];
			type A209 = A208[];
			type A210 = A209[];
			type A211 = A210[];
			type A212 = A211[];
			type A213 = A212[];
			type A214 = A213[];
			type A215 = A214[];
			type A216 = A215[];
			type A217 = A216[];
			type A218 = A217[];
			type A219 = A218[];
			type A220 = A219[];
			type A221 = A220[];
			type A222 = A221[];
			type A223 = A222[];
			type A224 = A223[];
			type A225 = A224[];
			type A226 = A225[];
			type A227 = A226[];
			type A228 = A227[];
			type A229 = A228[];
			type A230 = A229[];
			type A231 = A230[];
			type A232 = A231[];
			type A233 = A232[];
			type A234 = A233[];
			type A235 = A234[];
			type A236 = A235[];
			type A237 = A236[];
			type A238 = A237[];
			type A239 = A238[];
			type A240 = A239[];
			type A241 = A240[];
			type A242 = A241[];
			type A243 = A242[];
			type A244 = A243[];
			type A245 = A244[];
			type A246 = A245[];
			type A247 = A246[];
			type A248 = A247[];
			type A249 = A248[];
			type A250 = A249[];
			type A251 = A250[];
			type A252 = A251[];
			type A253 = A252[];
			type A254 = A253[];
			type A255 = A254[];
			type A256 = A255[];
			type A257 = A256[];
			type A258 = A257[];
			type A259 = A258[];
			type A260 = A259[];
			type A261 = A260[];
			type A262 = A261[];
			type A263 = A262[];
			type A264 = A263[];
			type A265 = A264[];
			type A266 = A265[];
			type A267 = A266[];
			type A268 = A267[];
			type A269 = A268[];
			type A270 = A269[];
			type A271 = A270[];
			type A272 = A271[];
			type A273 = A272[];
			type A274 = A273[];
			type A275 = A274[];
			type A276 = A275[];
			type A277 = A276[];
			type A278 = A277[];
			type A279 = A278[];
			type A280 = A279[];
			type A281 = A280[];
			type A282 = A281[];
			type A283 = A282[];
			type A284 = A283[];
			type A285 = A284[];
			type A286 = A285[];
			type A287 = A286[];
			type A288 = A287[];
			type A289 = A288[];
			type A290 = A289[];
			type A291 = A290[];
			type A292 = A291[];
			type A293 = A292[];
			type A294 = A293[];
			type A295 = A294[];
			type A296 = A295[];
			type A297 = A296[];
			type A298 = A297[];
			type A299 = A298[];
			type A300 = A299[];
			type A301 = A300[];
			type A302 = A301[];
			type A303 = A302[];
			type A304 = A303[];
			type A305 = A304[];
			type A306 = A305[];
			type A307 = A306[];
			type A308 = A307[];
			type A309 = A308[];
			type A310 = A309[];
			type A311 = A310[];
			type A312 = A311[];
			type A313 = A312[];
			type A314 = A313[];
			type A315 = A314[];
			type A316 = A315[];
			type A317 = A316[];
			type A318 = A317[];
			type A319 = A318[];
			type A320 = A319[];
			type A321 = A320[];
			type A322 = A321[];
			type A323 = A322[];
			type A324 = A323[];
			type A325 = A324[];
			type A326 = A325[];
			type A327 = A326[];
			type A328 = A327[];
			type A329 = A328[];
			type A330 = A329[];
			type A331 = A330[];
			type A332 = A331[];
			type A333 = A332[];
			type A334 = A333[];
			type A335 = A334[];
			type A336 = A335[];
			type A337 = A336[];
			type A338 = A337[];
			type A339 = A338[];
			type A340 = A339[];
			type A341 = A340[];
			type A342 = A341[];
			type A343 = A342[];
			type A344 = A343[];
			type A345 = A344[];
			type A346 = A345[];
			type A347 = A346[];
			type A348 = A347[];
			type A349 = A348[];
			type A350 = A349[];
			type A351 = A350[];
			type A352 = A351[];
			type A353 = A352[];
			type A354 = A353[];
			type A355 = A354[];
			type A356 = A355[];
			type A357 = A356[];
			type A358 = A357[];
			type A359 = A358[];
			type A360 = A359[];
			type A361 = A360[];
			type A362 = A361[];
			type A363 = A362[];
			type A364 = A363[];
			type A365 = A364[];
			type A366 = A365[];
			type A367 = A366[];
			type A368 = A367[];
			type A369 = A368[];
			type A370 = A369[];
			type A371 = A370[];
			type A372 = A371[];
			type A373 = A372[];
			type A374 = A373[];
			type A375 = A374[];
			type A376 = A375[];
			type A377 = A376[];
			type A378 = A377[];
			type A379 = A378[];
			type A380 = A379[];
			type A381 = A380[];
			type A382 = A381[];
			type A383 = A382[];
			type A384 = A383[];
			type A385 = A384[];
			type A386 = A385[];
			type A387 = A386[];
			type A388 = A387[];
			type A389 = A388[];
			type A390 = A389[];
			type A391 = A390[];
			type A392 = A391[];
			type A393 = A392[];
			type A394 = A393[];
			type A395 = A394[];
			type A396 = A395[];
			type A397 = A396[];
			type A398 = A397[];
			type A399 = A398[];
			type A400 = A399[];
			type A401 = A400[];
			type A402 = A401[];
			type A403 = A402[];
			type A404 = A403[];
			type A405 = A404[];
			type A406 = A405[];
			type A407 = A406[];
			type A408 = A407[];
			type A409 = A408[];
			type A410 = A409[];
			type A411 = A410[];
			type A412 = A411[];
			type A413 = A412[];
			type A414 = A413[];
			type A415 = A414[];
			type A416 = A415[];
			type A417 = A416[];
			type A418 = A417[];
			type A419 = A418[];
			type A420 = A419[];
			type A421 = A420[];
			type A422 = A421[];
			type A423 = A422[];
			type A424 = A423[];
			type A425 = A424[];
			type A426 = A425[];
			type A427 = A426[];
			type A428 = A427[];
			type A429 = A428[];
			type A430 = A429[];
			type A431 = A430[];
			type A432 = A431[];
			type A433 = A432[];
			type A434 = A433[];
			type A435 = A434[];
			type A436 = A435[];
			type A437 = A436[];
			type A438 = A437[];
			type A439 = A438[];
			type A440 = A439[];
			type A441 = A440[];
			type A442 = A441[];
			type A443 = A442[];
			type A444 = A443[];
			type A445 = A444[];
			type A446 = A445[];
			type A447 = A446[];
			type A448 = A447[];
			type A449 = A448[];
			type A450 = A449[];
			type A451 = A450[];
			type A452 = A451[];
			type A453 = A452[];
			type A454 = A453[];
			type A455 = A454[];
			type A456 = A455[];
			type A457 = A456[];
			type A458 = A457[];
			type A459 = A458[];
			type A460 = A459[];
			type A461 = A460[];
			type A462 = A461[];
			type A463 = A462[];
			type A464 = A463[];
			type A465 = A464[];
			type A466 = A465[];
			type A467 = A466[];
			type A468 = A467[];
			type A469 = A468[];
			type A470 = A469[];
			type A471 = A470[];
			type A472 = A471[];
			type A473 = A472[];
			type A474 = A473[];
			type A475 = A474[];
			type A476 = A475[];
			type A477 = A476[];
			type A478 = A477[];
			type A479 = A478[];
			type A480 = A479[];
			type A481 = A480[];
			type A482 = A481[];
			type A483 = A482[];
			type A484 = A483[];
			type A485 = A484[];
			type A486 = A485[];
			type A487 = A486[];
			type A488 = A487[];
			type A489 = A488[];
			type A490 = A489[];
			type A491 = A490[];
			type A492 = A491[];
			type A493 = A492[];
			type A494 = A493[];
			type A495 = A494[];
			type A496 = A495[];
			type A497 = A496[];
			type A498 = A497[];
			type A499 = A498[];
			type A500 = A499[];
			type A501 = A500[];
			type A502 = A501[];
			type A503 = A502[];
			type A504 = A503[];
			type A505 = A504[];
			type A506 = A505[];
			type A507 = A506[];
			type A508 = A507[];
			type A509 = A508[];
			type A510 = A509[];
			type A511 = A510[];
			type A512 = A511[];
			type A513 = A512[];
			type A514 = A513[];
			type A515 = A514[];
			type A516 = A515[];
			type A517 = A516[];
			type A518 = A517[];
			type A519 = A518[];
			type A520 = A519[];
			type A521 = A520[];
			type A522 = A521[];
			type A523 = A522[];
			type A524 = A523[];
			type A525 = A524[];
			type A526 = A525[];
			type A527 = A526[];
			type A528 = A527[];
			type A529 = A528[];
			type A530 = A529[];
			type A531 = A530[];
			type A532 = A531[];
			type A533 = A532[];
			type A534 = A533[];
			type A535 = A534[];
			type A536 = A535[];
			type A537 = A536[];
			type A538 = A537[];
			type A539 = A538[];
			type A540 = A539[];
			type A541 = A540[];
			type A542 = A541[];
			type A543 = A542[];
			type A544 = A543[];
			type A545 = A544[];
			type A546 = A545[];
			type A547 = A546[];
			type A548 = A547[];
			type A549 = A548[];
			type A550 = A549[];
			type A551 = A550[];
			type A552 = A551[];
			type A553 = A552[];
			type A554 = A553[];
			type A555 = A554[];
			type A556 = A555[];
			type A557 = A556[];
			type A558 = A557[];
			type A559 = A558[];
			type A560 = A559[];
			type A561 = A560[];
			type A562 = A561[];
			type A563 = A562[];
			type A564 = A563[];
			type A565 = A564[];
			type A566 = A565[];
			type A567 = A566[];
			type A568 = A567[];
			type A569 = A568[];
			type A570 = A569[];
			type A571 = A570[];
			type A572 = A571[];
			type A573 = A572[];
			type A574 = A573[];
			type A575 = A574[];
			type A576 = A575[];
			type A577 = A576[];
			type A578 = A577[];
			type A579 = A578[];
			type A580 = A579[];
			type A581 = A580[];
			type A582 = A581[];
			type A583 = A582[];
			type A584 = A583[];
			type A585 = A584[];
			type A586 = A585[];
			type A587 = A586[];
			type A588 = A587[];
			type A589 = A588[];
			type A590 = A589[];
			type A591 = A590[];
			type A592 = A591[];
			type A593 = A592[];
			type A594 = A593[];
			type A595 = A594[];
			type A596 = A595[];
			type A597 = A596[];
			type A598 = A597[];
			type A599 = A598[];
			type A600 = A599[];
			type A601 = A600[];
			type A602 = A601[];
			type A603 = A602[];
			type A604 = A603[];
			type A605 = A604[];
			type A606 = A605[];
			type A607 = A606[];
			type A608 = A607[];
			type A609 = A608[];
			type A610 = A609[];
			type A611 = A610[];
			type A612 = A611[];
			type A613 = A612[];
			type A614 = A613[];
			type A615 = A614[];
			type A616 = A615[];
			type A617 = A616[];
			type A618 = A617[];
			type A619 = A618[];
			type A620 = A619[];
			type A621 = A620[];
			type A622 = A621[];
			type A623 = A622[];
			type A624 = A623[];
			type A625 = A624[];
			type A626 = A625[];
			type A627 = A626[];
			type A628 = A627[];
			type A629 = A628[];
			type A630 = A629[];
			type A631 = A630[];
			type A632 = A631[];
			type A633 = A632[];
			type A634 = A633[];
			type A635 = A634[];
			type A636 = A635[];
			type A637 = A636[];
			type A638 = A637[];
			type A639 = A638[];
			type A640 = A639[];
			type A641 = A640[];
			type A642 = A641[];
			type A643 = A642[];
			type A644 = A643[];
			type A645 = A644[];
			type A646 = A645[];
			type A647 = A646[];
			type A648 = A647[];
			type A649 = A648[];
			type A650 = A649[];
			type A651 = A650[];
			type A652 = A651[];
			type A653 = A652[];
			type A654 = A653[];
			type A655 = A654[];
			type A656 = A655[];
			type A657 = A656[];
			type A658 = A657[];
			type A659 = A658[];
			type A660 = A659[];
			type A661 = A660[];
			type A662 = A661[];
			type A663 = A662[];
			type A664 = A663[];
			type A665 = A664[];
			type A666 = A665[];
			type A667 = A666[];
			type A668 = A667[];
			type A669 = A668[];
			type A670 = A669[];
			type A671 = A670[];
			type A672 = A671[];
			type A673 = A672[];
			type A674 = A673[];
			type A675 = A674[];
			type A676 = A675[];
			type A677 = A676[];
			type A678 = A677[];
			type A679 = A678[];
			type A680 = A679[];
			type A681 = A680[];
			type A682 = A681[];
			type A683 = A682[];
			type A684 = A683[];
			type A685 = A684[];
			type A686 = A685[];
			type A687 = A686[];
			type A688 = A687[];
			type A689 = A688[];
			type A690 = A689[];
			type A691 = A690[];
			type A692 = A691[];
			type A693 = A692[];
			type A694 = A693[];
			type A695 = A694[];
			type A696 = A695[];
			type A697 = A696[];
			type A698 = A697[];
			type A699 = A698[];
			type A700 = A699[];
			type A701 = A700[];
			type A702 = A701[];
			type A703 = A702[];
			type A704 = A703[];
			type A705 = A704[];
			type A706 = A705[];
			type A707 = A706[];
			type A708 = A707[];
			type A709 = A708[];
			type A710 = A709[];
			type A711 = A710[];
			type A712 = A711[];
			type A713 = A712[];
			type A714 = A713[];
			type A715 = A714[];
			type A716 = A715[];
			type A717 = A716[];
			type A718 = A717[];
			type A719 = A718[];
			type A720 = A719[];
			type A721 = A720[];
			type A722 = A721[];
			type A723 = A722[];
			type A724 = A723[];
			type A725 = A724[];
			type A726 = A725[];
			type A727 = A726[];
			type A728 = A727[];
			type A729 = A728[];
			type A730 = A729[];
			type A731 = A730[];
			type A732 = A731[];
			type A733 = A732[];
			type A734 = A733[];
			type A735 = A734[];
			type A736 = A735[];
			type A737 = A736[];
			type A738 = A737[];
			type A739 = A738[];
			type A740 = A739[];
			type A741 = A740[];
			type A742 = A741[];
			type A743 = A742[];
			type A744 = A743[];
			type A745 = A744[];
			type A746 = A745[];
			type A747 = A746[];
			type A748 = A747[];
			type A749 = A748[];
			type A750 = A749[];
			type A751 = A750[];
			type A752 = A751[];
			type A753 = A752[];
			type A754 = A753[];
			type A755 = A754[];
			type A756 = A755[];
			type A757 = A756[];
			type A758 = A757[];
			type A759 = A758[];
			type A760 = A759[];
			type A761 = A760[];
			type A762 = A761[];
			type A763 = A762[];
			type A764 = A763[];
			type A765 = A764[];
			type A766 = A765[];
			type A767 = A766[];
			type A768 = A767[];
			type A769 = A768[];
			type A770 = A769[];
			type A771 = A770[];
			type A772 = A771[];
			type A773 = A772[];
			type A774 = A773[];
			type A775 = A774[];
			type A776 = A775[];
			type A777 = A776[];
			type A778 = A777[];
			type A779 = A778[];
			type A780 = A779[];
			type A781 = A780[];
			type A782 = A781[];
			type A783 = A782[];
			type A784 = A783[];
			type A785 = A784[];
			type A786 = A785[];
			type A787 = A786[];
			type A788 = A787[];
			type A789 = A788[];
			type A790 = A789[];
			type A791 = A790[];
			type A792 = A791[];
			type A793 = A792[];
			type A794 = A793[];
			type A795 = A794[];
			type A796 = A795[];
			type A797 = A796[];
			type A798 = A797[];
			type A799 = A798[];
			type A800 = A799[];
			type A801 = A800[];
			type A802 = A801[];
			type A803 = A802[];
			type A804 = A803[];
			type A805 = A804[];
			type A806 = A805[];
			type A807 = A806[];
			type A808 = A807[];
			type A809 = A808[];
			type A810 = A809[];
			type A811 = A810[];
			type A812 = A811[];
			type A813 = A812[];
			type A814 = A813[];
			type A815 = A814[];
			type A816 = A815[];
			type A817 = A816[];
			type A818 = A817[];
			type A819 = A818[];
			type A820 = A819[];
			type A821 = A820[];
			type A822 = A821[];
			type A823 = A822[];
			type A824 = A823[];
			type A825 = A824[];
			type A826 = A825[];
			type A827 = A826[];
			type A828 = A827[];
			type A829 = A828[];
			type A830 = A829[];
			type A831 = A830[];
			type A832 = A831[];
			type A833 = A832[];
			type A834 = A833[];
			type A835 = A834[];
			type A836 = A835[];
			type A837 = A836[];
			type A838 = A837[];
			type A839 = A838[];
			type A840 = A839[];
			type A841 = A840[];
			type A842 = A841[];
			type A843 = A842[];
			type A844 = A843[];
			type A845 = A844[];
			type A846 = A845[];
			type A847 = A846[];
			type A848 = A847[];
			type A849 = A848[];
			type A850 = A849[];
			type A851 = A850[];
			type A852 = A851[];
			type A853 = A852[];
			type A854 = A853[];
			type A855 = A854[];
			type A856 = A855[];
			type A857 = A856[];
			type A858 = A857[];
			type A859 = A858[];
			type A860 = A859[];
			type A861 = A860[];
			type A862 = A861[];
			type A863 = A862[];
			type A864 = A863[];
			type A865 = A864[];
			type A866 = A865[];
			type A867 = A866[];
			type A868 = A867[];
			type A869 = A868[];
			type A870 = A869[];
			type A871 = A870[];
			type A872 = A871[];
			type A873 = A872[];
			type A874 = A873[];
			type A875 = A874[];
			type A876 = A875[];
			type A877 = A876[];
			type A878 = A877[];
			type A879 = A878[];
			type A880 = A879[];
			type A881 = A880[];
			type A882 = A881[];
			type A883 = A882[];
			type A884 = A883[];
			type A885 = A884[];
			type A886 = A885[];
			type A887 = A886[];
			type A888 = A887[];
			type A889 = A888[];
			type A890 = A889[];
			type A891 = A890[];
			type A892 = A891[];
			type A893 = A892[];
			type A894 = A893[];
			type A895 = A894[];
			type A896 = A895[];
			type A897 = A896[];
			type A898 = A897[];
			type A899 = A898[];
			type A900 = A899[];
			type A901 = A900[];
			type A902 = A901[];
			type A903 = A902[];
			type A904 = A903[];
			type A905 = A904[];
			type A906 = A905[];
			type A907 = A906[];
			type A908 = A907[];
			type A909 = A908[];
			type A910 = A909[];
			type A911 = A910[];
			type A912 = A911[];
			type A913 = A912[];
			type A914 = A913[];
			type A915 = A914[];
			type A916 = A915[];
			type A917 = A916[];
			type A918 = A917[];
			type A919 = A918[];
			type A920 = A919[];
			type A921 = A920[];
			type A922 = A921[];
			type A923 = A922[];
			type A924 = A923[];
			type A925 = A924[];
			type A926 = A925[];
			type A927 = A926[];
			type A928 = A927[];
			type A929 = A928[];
			type A930 = A929[];
			type A931 = A930[];
			type A932 = A931[];
			type A933 = A932[];
			type A934 = A933[];
			type A935 = A934[];
			type A936 = A935[];
			type A937 = A936[];
			type A938 = A937[];
			type A939 = A938[];
			type A940 = A939[];
			type A941 = A940[];
			type A942 = A941[];
			type A943 = A942[];
			type A944 = A943[];
			type A945 = A944[];
			type A946 = A945[];
			type A947 = A946[];
			type A948 = A947[];
			type A949 = A948[];
			type A950 = A949[];
			type A951 = A950[];
			type A952 = A951[];
			type A953 = A952[];
			type A954 = A953[];
			type A955 = A954[];
			type A956 = A955[];
			type A957 = A956[];
			type A958 = A957[];
			type A959 = A958[];
			type A960 = A959[];
			type A961 = A960[];
			type A962 = A961[];
			type A963 = A962[];
			type A964 = A963[];
			type A965 = A964[];
			type A966 = A965[];
			type A967 = A966[];
			type A968 = A967[];
			type A969 = A968[];
			type A970 = A969[];
			type A971 = A970[];
			type A972 = A971[];
			type A973 = A972[];
			type A974 = A973[];
			type A975 = A974[];
			type A976 = A975[];
			type A977 = A976[];
			type A978 = A977[];
			type A979 = A978[];
			type A980 = A979[];
			type A981 = A980[];
			type A982 = A981[];
			type A983 = A982[];
			type A984 = A983[];
			type A985 = A984[];
			type A986 = A985[];
			type A987 = A986[];
			type A988 = A987[];
			type A989 = A988[];
			type A990 = A989[];
			type A991 = A990[];
			type A992 = A991[];
			type A993 = A992[];
			type A994 = A993[];
			type A995 = A994[];
			type A996 = A995[];
			type A997 = A996[];
			type A998 = A997[];
			type A999 = A998[];
			type A1000 = A999[];
			type A1001 = A1000[];
			type A1002 = A1001[];
			type A1003 = A1002[];
			type A1004 = A1003[];
			type A1005 = A1004[];
			type A1006 = A1005[];
			type A1007 = A1006[];
			type A1008 = A1007[];
			type A1009 = A1008[];
			type A1010 = A1009[];
			type A1011 = A1010[];
			type A1012 = A1011[];
			type A1013 = A1012[];
			type A1014 = A1013[];
			type A1015 = A1014[];
			type A1016 = A1015[];
			type A1017 = A1016[];
			type A1018 = A1017[];
			type A1019 = A1018[];
			type A1020 = A1019[];
			type A1021 = A1020[];
			type A1022 = A1021[];
			type A1023 = A1022[];
			type A1024 = A1023[];
			type A1025 = A1024[];
			type A1026 = A1025[];
			type A1027 = A1026[];
			type A1028 = A1027[];
			type A1029 = A1028[];
			type A1030 = A1029[];
			type A1031 = A1030[];
			type A1032 = A1031[];
			type A1033 = A1032[];
			type A1034 = A1033[];
			type A1035 = A1034[];
			type A1036 = A1035[];
			type A1037 = A1036[];
			type A1038 = A1037[];
			type A1039 = A1038[];
			type A1040 = A1039[];
			type A1041 = A1040[];
			type A1042 = A1041[];
			type A1043 = A1042[];
			type A1044 = A1043[];
			type A1045 = A1044[];
			type A1046 = A1045[];
			type A1047 = A1046[];
			type A1048 = A1047[];
			type A1049 = A1048[];
			type A1050 = A1049[];
			type A1051 = A1050[];
			type A1052 = A1051[];
			type A1053 = A1052[];
			type A1054 = A1053[];
			type A1055 = A1054[];
			type A1056 = A1055[];
			type A1057 = A1056[];
			type A1058 = A1057[];
			type A1059 = A1058[];
			type A1060 = A1059[];
			type A1061 = A1060[];
			type A1062 = A1061[];
			type A1063 = A1062[];
			type A1064 = A1063[];
			type A1065 = A1064[];
			type A1066 = A1065[];
			type A1067 = A1066[];
			type A1068 = A1067[];
			type A1069 = A1068[];
			type A1070 = A1069[];
			type A1071 = A1070[];
			type A1072 = A1071[];
			type A1073 = A1072[];
			type A1074 = A1073[];
			type A1075 = A1074[];
			type A1076 = A1075[];
			type A1077 = A1076[];
			type A1078 = A1077[];
			type A1079 = A1078[];
			type A1080 = A1079[];
			type A1081 = A1080[];
			type A1082 = A1081[];
			type A1083 = A1082[];
			type A1084 = A1083[];
			type A1085 = A1084[];
			type A1086 = A1085[];
			type A1087 = A1086[];
			type A1088 = A1087[];
			type A1089 = A1088[];
			type A1090 = A1089[];
			type A1091 = A1090[];
			type A1092 = A1091[];
			type A1093 = A1092[];
			type A1094 = A1093[];
			type A1095 = A1094[];
			type A1096 = A1095[];
			type A1097 = A1096[];
			type A1098 = A1097[];
			type A1099 = A1098[];
			type A1100 = A1099[];
			type A1101 = A1100[];
			type A1102 = A1101[];
			type A1103 = A1102[];
			type A1104 = A1103[];
			type A1105 = A1104[];
			type A1106 = A1105[];
			type A1107 = A1106[];
			type A1108 = A1107[];
			type A1109 = A1108[];
			type A1110 = A1109[];
			type A1111 = A1110[];
			type A1112 = A1111[];
			type A1113 = A1112[];
			type A1114 = A1113[];
			type A1115 = A1114[];
			type A1116 = A1115[];
			type A1117 = A1116[];
			type A1118 = A1117[];
			type A1119 = A1118[];
			type A1120 = A1119[];
			type A1121 = A1120[];
			type A1122 = A1121[];
			type A1123 = A1122[];
			type A1124 = A1123[];
			type A1125 = A1124[];
			type A1126 = A1125[];
			type A1127 = A1126[];
			type A1128 = A1127[];
			type A1129 = A1128[];
			type A1130 = A1129[];
			type A1131 = A1130[];
			type A1132 = A1131[];
			type A1133 = A1132[];
			type A1134 = A1133[];
			type A1135 = A1134[];
			type A1136 = A1135[];
			type A1137 = A1136[];
			type A1138 = A1137[];
			type A1139 = A1138[];
			type A1140 = A1139[];
			type A1141 = A1140[];
			type A1142 = A1141[];
			type A1143 = A1142[];
			type A1144 = A1143[];
			type A1145 = A1144[];
			type A1146 = A1145[];
			type A1147 = A1146[];
			type A1148 = A1147[];
			type A1149 = A1148[];
			type A1150 = A1149[];
			type A1151 = A1150[];
			type A1152 = A1151[];
			type A1153 = A1152[];
			type A1154 = A1153[];
			type A1155 = A1154[];
			type A1156 = A1155[];
			type A1157 = A1156[];
			type A1158 = A1157[];
			type A1159 = A1158[];
			type A1160 = A1159[];
			type A1161 = A1160[];
			type A1162 = A1161[];
			type A1163 = A1162[];
			type A1164 = A1163[];
			type A1165 = A1164[];
			type A1166 = A1165[];
			type A1167 = A1166[];
			type A1168 = A1167[];
			type A1169 = A1168[];
			type A1170 = A1169[];
			type A1171 = A1170[];
			type A1172 = A1171[];
			type A1173 = A1172[];
			type A1174 = A1173[];
			type A1175 = A1174[];
			type A1176 = A1175[];
			type A1177 = A1176[];
			type A1178 = A1177[];
			type A1179 = A1178[];
			type A1180 = A1179[];
			type A1181 = A1180[];
			type A1182 = A1181[];
			type A1183 = A1182[];
			type A1184 = A1183[];
			type A1185 = A1184[];
			type A1186 = A1185[];
			type A1187 = A1186[];
			type A1188 = A1187[];
			type A1189 = A1188[];
			type A1190 = A1189[];
			type A1191 = A1190[];
			type A1192 = A1191[];
			type A1193 = A1192[];
			type A1194 = A1193[];
			type A1195 = A1194[];
			type A1196 = A1195[];
			type A1197 = A1196[];
			type A1198 = A1197[];
			type A1199 = A1198[];
			type A1200 = A1199[];
			type A1201 = A1200[];
			type A1202 = A1201[];
			type A1203 = A1202[];
			type A1204 = A1203[];
			type A1205 = A1204[];
			type A1206 = A1205[];
			type A1207 = A1206[];
			type A1208 = A1207[];
			type A1209 = A1208[];
			type A1210 = A1209[];
			type A1211 = A1210[];
			type A1212 = A1211[];
			type A1213 = A1212[];
			type A1214 = A1213[];
			type A1215 = A1214[];
			type A1216 = A1215[];
			type A1217 = A1216[];
			type A1218 = A1217[];
			type A1219 = A1218[];
			type A1220 = A1219[];
			type A1221 = A1220[];
			type A1222 = A1221[];
			type A1223 = A1222[];
			type A1224 = A1223[];
			type A1225 = A1224[];
			type A1226 = A1225[];
			type A1227 = A1226[];
			type A1228 = A1227[];
			type A1229 = A1228[];
			type A1230 = A1229[];
			type A1231 = A1230[];
			type A1232 = A1231[];
			type A1233 = A1232[];
			type A1234 = A1233[];
			type A1235 = A1234[];
			type A1236 = A1235[];
			type A1237 = A1236[];
			type A1238 = A1237[];
			type A1239 = A1238[];
			type A1240 = A1239[];
			type A1241 = A1240[];
			type A1242 = A1241[];
			type A1243 = A1242[];
			type A1244 = A1243[];
			type A1245 = A1244[];
			type A1246 = A1245[];
			type A1247 = A1246[];
			type A1248 = A1247[];
			type A1249 = A1248[];
			type A1250 = A1249[];
			type A1251 = A1250[];
			type A1252 = A1251[];
			type A1253 = A1252[];
			type A1254 = A1253[];
			type A1255 = A1254[];
			type A1256 = A1255[];
			type A1257 = A1256[];
			type A1258 = A1257[];
			type A1259 = A1258[];
			type A1260 = A1259[];
			type A1261 = A1260[];
			type A1262 = A1261[];
			type A1263 = A1262[];
			type A1264 = A1263[];
			type A1265 = A1264[];
			type A1266 = A1265[];
			type A1267 = A1266[];
			type A1268 = A1267[];
			type A1269 = A1268[];
			type A1270 = A1269[];
			type A1271 = A1270[];
			type A1272 = A1271[];
			type A1273 = A1272[];
			type A1274 = A1273[];
			type A1275 = A1274[];
			type A1276 = A1275[];
			type A1277 = A1276[];
			type A1278 = A1277[];
			type A1279 = A1278[];
			type A1280 = A1279[];
			type A1281 = A1280[];
			type A1282 = A1281[];
			type A1283 = A1282[];
			type A1284 = A1283[];
			type A1285 = A1284[];
			type A1286 = A1285[];
			type A1287 = A1286[];
			type A1288 = A1287[];
			type A1289 = A1288[];
			type A1290 = A1289[];
			type A1291 = A1290[];
			type A1292 = A1291[];
			type A1293 = A1292[];
			type A1294 = A1293[];
			type A1295 = A1294[];
			type A1296 = A1295[];
			type A1297 = A1296[];
			type A1298 = A1297[];
			type A1299 = A1298[];
			type A1300 = A1299[];
			type A1301 = A1300[];
			type A1302 = A1301[];
			type A1303 = A1302[];
			type A1304 = A1303[];
			type A1305 = A1304[];
			type A1306 = A1305[];
			type A1307 = A1306[];
			type A1308 = A1307[];
			type A1309 = A1308[];
			type A1310 = A1309[];
			type A1311 = A1310[];
			type A1312 = A1311[];
			type A1313 = A1312[];
			type A1314 = A1313[];
			type A1315 = A1314[];
			type A1316 = A1315[];
			type A1317 = A1316[];
			type A1318 = A1317[];
			type A1319 = A1318[];
			type A1320 = A1319[];
			type A1321 = A1320[];
			type A1322 = A1321[];
			type A1323 = A1322[];
			type A1324 = A1323[];
			type A1325 = A1324[];
			type A1326 = A1325[];
			type A1327 = A1326[];
			type A1328 = A1327[];
			type A1329 = A1328[];
			type A1330 = A1329[];
			type A1331 = A1330[];
			type A1332 = A1331[];
			type A1333 = A1332[];
			type A1334 = A1333[];
			type A1335 = A1334[];
			type A1336 = A1335[];
			type A1337 = A1336[];
			type A1338 = A1337[];
			type A1339 = A1338[];
			type A1340 = A1339[];
			type A1341 = A1340[];
			type A1342 = A1341[];
			type A1343 = A1342[];
			type A1344 = A1343[];
			type A1345 = A1344[];
			type A1346 = A1345[];
			type A1347 = A1346[];
			type A1348 = A1347[];
			type A1349 = A1348[];
			type A1350 = A1349[];
			type A1351 = A1350[];
			type A1352 = A1351[];
			type A1353 = A1352[];
			type A1354 = A1353[];
			type A1355 = A1354[];
			type A1356 = A1355[];
			type A1357 = A1356[];
			type A1358 = A1357[];
			type A1359 = A1358[];
			type A1360 = A1359[];
			type A1361 = A1360[];
			type A1362 = A1361[];
			type A1363 = A1362[];
			type A1364 = A1363[];
			type A1365 = A1364[];
			type A1366 = A1365[];
			type A1367 = A1366[];
			type A1368 = A1367[];
			type A1369 = A1368[];
			type A1370 = A1369[];
			type A1371 = A1370[];
			type A1372 = A1371[];
			type A1373 = A1372[];
			type A1374 = A1373[];
			type A1375 = A1374[];
			type A1376 = A1375[];
			type A1377 = A1376[];
			type A1378 = A1377[];
			type A1379 = A1378[];
			type A1380 = A1379[];
			type A1381 = A1380[];
			type A1382 = A1381[];
			type A1383 = A1382[];
			type A1384 = A1383[];
			type A1385 = A1384[];
			type A1386 = A1385[];
			type A1387 = A1386[];
			type A1388 = A1387[];
			type A1389 = A1388[];
			type A1390 = A1389[];
			type A1391 = A1390[];
			type A1392 = A1391[];
			type A1393 = A1392[];
			type A1394 = A1393[];
			type A1395 = A1394[];
			type A1396 = A1395[];
			type A1397 = A1396[];
			type A1398 = A1397[];
			type A1399 = A1398[];
			type A1400 = A1399[];
			type A1401 = A1400[];
			type A1402 = A1401[];
			type A1403 = A1402[];
			type A1404 = A1403[];
			type A1405 = A1404[];
			type A1406 = A1405[];
			type A1407 = A1406[];
			type A1408 = A1407[];
			type A1409 = A1408[];
			type A1410 = A1409[];
			type A1411 = A1410[];
			type A1412 = A1411[];
			type A1413 = A1412[];
			type A1414 = A1413[];
			type A1415 = A1414[];
			type A1416 = A1415[];
			type A1417 = A1416[];
			type A1418 = A1417[];
			type A1419 = A1418[];
			type A1420 = A1419[];
			type A1421 = A1420[];
			type A1422 = A1421[];
			type A1423 = A1422[];
			type A1424 = A1423[];
			type A1425 = A1424[];
			type A1426 = A1425[];
			type A1427 = A1426[];
			type A1428 = A1427[];
			type A1429 = A1428[];
			type A1430 = A1429[];
			type A1431 = A1430[];
			type A1432 = A1431[];
			type A1433 = A1432[];
			type A1434 = A1433[];
			type A1435 = A1434[];
			type A1436 = A1435[];
			type A1437 = A1436[];
			type A1438 = A1437[];
			type A1439 = A1438[];
			type A1440 = A1439[];
			type A1441 = A1440[];
			type A1442 = A1441[];
			type A1443 = A1442[];
			type A1444 = A1443[];
			type A1445 = A1444[];
			type A1446 = A1445[];
			type A1447 = A1446[];
			type A1448 = A1447[];
			type A1449 = A1448[];
			type A1450 = A1449[];
			type A1451 = A1450[];
			type A1452 = A1451[];
			type A1453 = A1452[];
			type A1454 = A1453[];
			type A1455 = A1454[];
			type A1456 = A1455[];
			type A1457 = A1456[];
			type A1458 = A1457[];
			type A1459 = A1458[];
			type A1460 = A1459[];
			type A1461 = A1460[];
			type A1462 = A1461[];
			type A1463 = A1462[];
			type A1464 = A1463[];
			type A1465 = A1464[];
			type A1466 = A1465[];
			type A1467 = A1466[];
			type A1468 = A1467[];
			type A1469 = A1468[];
			type A1470 = A1469[];
			type A1471 = A1470[];
			type A1472 = A1471[];
			type A1473 = A1472[];
			type A1474 = A1473[];
			type A1475 = A1474[];
			type A1476 = A1475[];
			type A1477 = A1476[];
			type A1478 = A1477[];
			type A1479 = A1478[];
			type A1480 = A1479[];
			type A1481 = A1480[];
			type A1482 = A1481[];
			type A1483 = A1482[];
			type A1484 = A1483[];
			type A1485 = A1484[];
			type A1486 = A1485[];
			type A1487 = A1486[];
			type A1488 = A1487[];
			type A1489 = A1488[];
			type A1490 = A1489[];
			type A1491 = A1490[];
			type A1492 = A1491[];
			type A1493 = A1492[];
			type A1494 = A1493[];
			type A1495 = A1494[];
			type A1496 = A1495[];
			type A1497 = A1496[];
			type A1498 = A1497[];
			type A1499 = A1498[];
			type A1500 = A1499[];
			type A1501 = A1500[];
			type A1502 = A1501[];
			type A1503 = A1502[];
			type A1504 = A1503[];
			type A1505 = A1504[];
			type A1506 = A1505[];
			type A1507 = A1506[];
			type A1508 = A1507[];
			type A1509 = A1508[];
			type A1510 = A1509[];
			type A1511 = A1510[];
			type A1512 = A1511[];
			type A1513 = A1512[];
			type A1514 = A1513[];
			type A1515 = A1514[];
			type A1516 = A1515[];
			type A1517 = A1516[];
			type A1518 = A1517[];
			type A1519 = A1518[];
			type A1520 = A1519[];
			type A1521 = A1520[];
			type A1522 = A1521[];
			type A1523 = A1522[];
			type A1524 = A1523[];
			type A1525 = A1524[];
			type A1526 = A1525[];
			type A1527 = A1526[];
			type A1528 = A1527[];
			type A1529 = A1528[];
			type A1530 = A1529[];
			type A1531 = A1530[];
			type A1532 = A1531[];
			type A1533 = A1532[];
			type A1534 = A1533[];
			type A1535 = A1534[];
			type A1536 = A1535[];
			type A1537 = A1536[];
			type A1538 = A1537[];
			type A1539 = A1538[];
			type A1540 = A1539[];
			type A1541 = A1540[];
			type A1542 = A1541[];
			type A1543 = A1542[];
			type A1544 = A1543[];
			type A1545 = A1544[];
			type A1546 = A1545[];
			type A1547 = A1546[];
			type A1548 = A1547[];
			type A1549 = A1548[];
			type A1550 = A1549[];
			type A1551 = A1550[];
			type A1552 = A1551[];
			type A1553 = A1552[];
			type A1554 = A1553[];
			type A1555 = A1554[];
			type A1556 = A1555[];
			type A1557 = A1556[];
			type A1558 = A1557[];
			type A1559 = A1558[];
			type A1560 = A1559[];
			type A1561 = A1560[];
			type A1562 = A1561[];
			type A1563 = A1562[];
			type A1564 = A1563[];
			type A1565 = A1564[];
			type A1566 = A1565[];
			type A1567 = A1566[];
			type A1568 = A1567[];
			type A1569 = A1568[];
			type A1570 = A1569[];
			type A1571 = A1570[];
			type A1572 = A1571[];
			type A1573 = A1572[];
			type A1574 = A1573[];
			type A1575 = A1574[];
			type A1576 = A1575[];
			type A1577 = A1576[];
			type A1578 = A1577[];
			type A1579 = A1578[];
			type A1580 = A1579[];
			type A1581 = A1580[];
			type A1582 = A1581[];
			type A1583 = A1582[];
			type A1584 = A1583[];
			type A1585 = A1584[];
			type A1586 = A1585[];
			type A1587 = A1586[];
			type A1588 = A1587[];
			type A1589 = A1588[];
			type A1590 = A1589[];
			type A1591 = A1590[];
			type A1592 = A1591[];
			type A1593 = A1592[];
			type A1594 = A1593[];
			type A1595 = A1594[];
			type A1596 = A1595[];
			type A1597 = A1596[];
			type A1598 = A1597[];
			type A1599 = A1598[];
			type A1600 = A1599[];
			type A1601 = A1600[];
			type A1602 = A1601[];
			type A1603 = A1602[];
			type A1604 = A1603[];
			type A1605 = A1604[];
			type A1606 = A1605[];
			type A1607 = A1606[];
			type A1608 = A1607[];
			type A1609 = A1608[];
			type A1610 = A1609[];
			type A1611 = A1610[];
			type A1612 = A1611[];
			type A1613 = A1612[];
			type A1614 = A1613[];
			type A1615 = A1614[];
			type A1616 = A1615[];
			type A1617 = A1616[];
			type A1618 = A1617[];
			type A1619 = A1618[];
			type A1620 = A1619[];
			type A1621 = A1620[];
			type A1622 = A1621[];
			type A1623 = A1622[];
			type A1624 = A1623[];
			type A1625 = A1624[];
			type A1626 = A1625[];
			type A1627 = A1626[];
			type A1628 = A1627[];
			type A1629 = A1628[];
			type A1630 = A1629[];
			type A1631 = A1630[];
			type A1632 = A1631[];
			type A1633 = A1632[];
			type A1634 = A1633[];
			type A1635 = A1634[];
			type A1636 = A1635[];
			type A1637 = A1636[];
			type A1638 = A1637[];
			type A1639 = A1638[];
			type A1640 = A1639[];
			type A1641 = A1640[];
			type A1642 = A1641[];
			type A1643 = A1642[];
			type A1644 = A1643[];
			type A1645 = A1644[];
			type A1646 = A1645[];
			type A1647 = A1646[];
			type A1648 = A1647[];
			type A1649 = A1648[];
			type A1650 = A1649[];
			type A1651 = A1650[];
			type A1652 = A1651[];
			type A1653 = A1652[];
			type A1654 = A1653[];
			type A1655 = A1654[];
			type A1656 = A1655[];
			type A1657 = A1656[];
			type A1658 = A1657[];
			type A1659 = A1658[];
			type A1660 = A1659[];
			type A1661 = A1660[];
			type A1662 = A1661[];
			type A1663 = A1662[];
			type A1664 = A1663[];
			type A1665 = A1664[];
			type A1666 = A1665[];
			type A1667 = A1666[];
			type A1668 = A1667[];
			type A1669 = A1668[];
			type A1670 = A1669[];
			type A1671 = A1670[];
			type A1672 = A1671[];
			type A1673 = A1672[];
			type A1674 = A1673[];
			type A1675 = A1674[];
			type A1676 = A1675[];
			type A1677 = A1676[];
			type A1678 = A1677[];
			type A1679 = A1678[];
			type A1680 = A1679[];
			type A1681 = A1680[];
			type A1682 = A1681[];
			type A1683 = A1682[];
			type A1684 = A1683[];
			type A1685 = A1684[];
			type A1686 = A1685[];
			type A1687 = A1686[];
			type A1688 = A1687[];
			type A1689 = A1688[];
			type A1690 = A1689[];
			type A1691 = A1690[];
			type A1692 = A1691[];
			type A1693 = A1692[];
			type A1694 = A1693[];
			type A1695 = A1694[];
			type A1696 = A1695[];
			type A1697 = A1696[];
			type A1698 = A1697[];
			type A1699 = A1698[];
			type A1700 = A1699[];
			type A1701 = A1700[];
			type A1702 = A1701[];
			type A1703 = A1702[];
			type A1704 = A1703[];
			type A1705 = A1704[];
			type A1706 = A1705[];
			type A1707 = A1706[];
			type A1708 = A1707[];
			type A1709 = A1708[];
			type A1710 = A1709[];
			type A1711 = A1710[];
			type A1712 = A1711[];
			type A1713 = A1712[];
			type A1714 = A1713[];
			type A1715 = A1714[];
			type A1716 = A1715[];
			type A1717 = A1716[];
			type A1718 = A1717[];
			type A1719 = A1718[];
			type A1720 = A1719[];
			type A1721 = A1720[];
			type A1722 = A1721[];
			type A1723 = A1722[];
			type A1724 = A1723[];
			type A1725 = A1724[];
			type A1726 = A1725[];
			type A1727 = A1726[];
			type A1728 = A1727[];
			type A1729 = A1728[];
			type A1730 = A1729[];
			type A1731 = A1730[];
			type A1732 = A1731[];
			type A1733 = A1732[];
			type A1734 = A1733[];
			type A1735 = A1734[];
			type A1736 = A1735[];
			type A1737 = A1736[];
			type A1738 = A1737[];
			type A1739 = A1738[];
			type A1740 = A1739[];
			type A1741 = A1740[];
			type A1742 = A1741[];
			type A1743 = A1742[];
			type A1744 = A1743[];
			type A1745 = A1744[];
			type A1746 = A1745[];
			type A1747 = A1746[];
			type A1748 = A1747[];
			type A1749 = A1748[];
			type A1750 = A1749[];
			type A1751 = A1750[];
			type A1752 = A1751[];
			type A1753 = A1752[];
			type A1754 = A1753[];
			type A1755 = A1754[];
			type A1756 = A1755[];
			type A1757 = A1756[];
			type A1758 = A1757[];
			type A1759 = A1758[];
			type A1760 = A1759[];
			type A1761 = A1760[];
			type A1762 = A1761[];
			type A1763 = A1762[];
			type A1764 = A1763[];
			type A1765 = A1764[];
			type A1766 = A1765[];
			type A1767 = A1766[];
			type A1768 = A1767[];
			type A1769 = A1768[];
			type A1770 = A1769[];
			type A1771 = A1770[];
			type A1772 = A1771[];
			type A1773 = A1772[];
			type A1774 = A1773[];
			type A1775 = A1774[];
			type A1776 = A1775[];
			type A1777 = A1776[];
			type A1778 = A1777[];
			type A1779 = A1778[];
			type A1780 = A1779[];
			type A1781 = A1780[];
			type A1782 = A1781[];
			type A1783 = A1782[];
			type A1784 = A1783[];
			type A1785 = A1784[];
			type A1786 = A1785[];
			type A1787 = A1786[];
			type A1788 = A1787[];
			type A1789 = A1788[];
			type A1790 = A1789[];
			type A1791 = A1790[];
			type A1792 = A1791[];
			type A1793 = A1792[];
			type A1794 = A1793[];
			type A1795 = A1794[];
			type A1796 = A1795[];
			type A1797 = A1796[];
			type A1798 = A1797[];
			type A1799 = A1798[];
			type A1800 = A1799[];
			type A1801 = A1800[];
			type A1802 = A1801[];
			type A1803 = A1802[];
			type A1804 = A1803[];
			type A1805 = A1804[];
			type A1806 = A1805[];
			type A1807 = A1806[];
			type A1808 = A1807[];
			type A1809 = A1808[];
			type A1810 = A1809[];
			type A1811 = A1810[];
			type A1812 = A1811[];
			type A1813 = A1812[];
			type A1814 = A1813[];
			type A1815 = A1814[];
			type A1816 = A1815[];
			type A1817 = A1816[];
			type A1818 = A1817[];
			type A1819 = A1818[];
			type A1820 = A1819[];
			type A1821 = A1820[];
			type A1822 = A1821[];
			type A1823 = A1822[];
			type A1824 = A1823[];
			type A1825 = A1824[];
			type A1826 = A1825[];
			type A1827 = A1826[];
			type A1828 = A1827[];
			type A1829 = A1828[];
			type A1830 = A1829[];
			type A1831 = A1830[];
			type A1832 = A1831[];
			type A1833 = A1832[];
			type A1834 = A1833[];
			type A1835 = A1834[];
			type A1836 = A1835[];
			type A1837 = A1836[];
			type A1838 = A1837[];
			type A1839 = A1838[];
			type A1840 = A1839[];
			type A1841 = A1840[];
			type A1842 = A1841[];
			type A1843 = A1842[];
			type A1844 = A1843[];
			type A1845 = A1844[];
			type A1846 = A1845[];
			type A1847 = A1846[];
			type A1848 = A1847[];
			type A1849 = A1848[];
			type A1850 = A1849[];
			type A1851 = A1850[];
			type A1852 = A1851[];
			type A1853 = A1852[];
			type A1854 = A1853[];
			type A1855 = A1854[];
			type A1856 = A1855[];
			type A1857 = A1856[];
			type A1858 = A1857[];
			type A1859 = A1858[];
			type A1860 = A1859[];
			type A1861 = A1860[];
			type A1862 = A1861[];
			type A1863 = A1862[];
			type A1864 = A1863[];
			type A1865 = A1864[];
			type A1866 = A1865[];
			type A1867 = A1866[];
			type A1868 = A1867[];
			type A1869 = A1868[];
			type A1870 = A1869[];
			type A1871 = A1870[];
			type A1872 = A1871[];
			type A1873 = A1872[];
			type A1874 = A1873[];
			type A1875 = A1874[];
			type A1876 = A1875[];
			type A1877 = A1876[];
			type A1878 = A1877[];
			type A1879 = A1878[];
			type A1880 = A1879[];
			type A1881 = A1880[];
			type A1882 = A1881[];
			type A1883 = A1882[];
			type A1884 = A1883[];
			type A1885 = A1884[];
			type A1886 = A1885[];
			type A1887 = A1886[];
			type A1888 = A1887[];
			type A1889 = A1888[];
			type A1890 = A1889[];
			type A1891 = A1890[];
			type A1892 = A1891[];
			type A1893 = A1892[];
			type A1894 = A1893[];
			type A1895 = A1894[];
			type A1896 = A1895[];
			type A1897 = A1896[];
			type A1898 = A1897[];
			type A1899 = A1898[];
			type A1900 = A1899[];
			type A1901 = A1900[];
			type A1902 = A1901[];
			type A1903 = A1902[];
			type A1904 = A1903[];
			type A1905 = A1904[];
			type A1906 = A1905[];
			type A1907 = A1906[];
			type A1908 = A1907[];
			type A1909 = A1908[];
			type A1910 = A1909[];
			type A1911 = A1910[];
			type A1912 = A1911[];
			type A1913 = A1912[];
			type A1914 = A1913[];
			type A1915 = A1914[];
			type A1916 = A1915[];
			type A1917 = A1916[];
			type A1918 = A1917[];
			type A1919 = A1918[];
			type A1920 = A1919[];
			type A1921 = A1920[];
			type A1922 = A1921[];
			type A1923 = A1922[];
			type A1924 = A1923[];
			type A1925 = A1924[];
			type A1926 = A1925[];
			type A1927 = A1926[];
			type A1928 = A1927[];
			type A1929 = A1928[];
			type A1930 = A1929[];
			type A1931 = A1930[];
			type A1932 = A1931[];
			type A1933 = A1932[];
			type A1934 = A1933[];
			type A1935 = A1934[];
			type A1936 = A1935[];
			type A1937 = A1936[];
			type A1938 = A1937[];
			type A1939 = A1938[];
			type A1940 = A1939[];
			type A1941 = A1940[];
			type A1942 = A1941[];
			type A1943 = A1942[];
			type A1944 = A1943[];
			type A1945 = A1944[];
			type A1946 = A1945[];
			type A1947 = A1946[];
			type A1948 = A1947[];
			type A1949 = A1948[];
			type A1950 = A1949[];
			type A1951 = A1950[];
			type A1952 = A1951[];
			type A1953 = A1952[];
			type A1954 = A1953[];
			type A1955 = A1954[];
			type A1956 = A1955[];
			type A1957 = A1956[];
			type A1958 = A1957[];
			type A1959 = A1958[];
			type A1960 = A1959[];
			type A1961 = A1960[];
			type A1962 = A1961[];
			type A1963 = A1962[];
			type A1964 = A1963[];
			type A1965 = A1964[];
			type A1966 = A1965[];
			type A1967 = A1966[];
			type A1968 = A1967[];
			type A1969 = A1968[];
			type A1970 = A1969[];
			type A1971 = A1970[];
			type A1972 = A1971[];
			type A1973 = A1972[];
			type A1974 = A1973[];
			type A1975 = A1974[];
			type A1976 = A1975[];
			type A1977 = A1976[];
			type A1978 = A1977[];
			type A1979 = A1978[];
			type A1980 = A1979[];
			type A1981 = A1980[];
			type A1982 = A1981[];
			type A1983 = A1982[];
			type A1984 = A1983[];
			type A1985 = A1984[];
			type A1986 = A1985[];
			type A1987 = A1986[];
			type A1988 = A1987[];
			type A1989 = A1988[];
			type A1990 = A1989[];
			type A1991 = A1990[];
			type A1992 = A1991[];
			type A1993 = A1992[];
			type A1994 = A1993[];
			type A1995 = A1994[];
			type A1996 = A1995[];
			type A1997 = A1996[];
			type A1998 = A1997[];
			type A1999 = A1998[];
			type A2000 = A1999[];
			type A2001 = A2000[];
			type A2002 = A2001[];
			type A2003 = A2002[];
			type A2004 = A2003[];
			type A2005 = A2004[];
			type A2006 = A2005[];
			type A2007 = A2006[];
			type A2008 = A2007[];
			type A2009 = A2008[];
			type A2010 = A2009[];
			type A2011 = A2010[];
			type A2012 = A2011[];
			type A2013 = A2012[];
			type A2014 = A2013[];
			type A2015 = A2014[];
			type A2016 = A2015[];
			type A2017 = A2016[];
			type A2018 = A2017[];
			type A2019 = A2018[];
			type A2020 = A2019[];
			type A2021 = A2020[];
			type A2022 = A2021[];
			type A2023 = A2022[];
			type A2024 = A2023[];
			type A2025 = A2024[];
			type A2026 = A2025[];
			type A2027 = A2026[];
			type A2028 = A2027[];
			type A2029 = A2028[];
			type A2030 = A2029[];
			type A2031 = A2030[];
			type A2032 = A2031[];
			type A2033 = A2032[];
			type A2034 = A2033[];
			type A2035 = A2034[];
			type A2036 = A2035[];
			type A2037 = A2036[];
			type A2038 = A2037[];
			type A2039 = A2038[];
			type A2040 = A2039[];
			type A2041 = A2040[];
			type A2042 = A2041[];
			type A2043 = A2042[];
			type A2044 = A2043[];
			type A2045 = A2044[];
			type A2046 = A2045[];
			type A2047 = A2046[];
			type A2048 = A2047[];
			type A2049 = A2048[];
			type A2050 = A2049[];
			type A2051 = A2050[];
			type A2052 = A2051[];
			type A2053 = A2052[];
			type A2054 = A2053[];
			type A2055 = A2054[];
			type A2056 = A2055[];
			type A2057 = A2056[];
			type A2058 = A2057[];
			type A2059 = A2058[];
			type A2060 = A2059[];
			type A2061 = A2060[];
			type A2062 = A2061[];
			type A2063 = A2062[];
			type A2064 = A2063[];
			type A2065 = A2064[];
			type A2066 = A2065[];
			type A2067 = A2066[];
			type A2068 = A2067[];
			type A2069 = A2068[];
			type A2070 = A2069[];
			type A2071 = A2070[];
			type A2072 = A2071[];
			type A2073 = A2072[];
			type A2074 = A2073[];
			type A2075 = A2074[];
			type A2076 = A2075[];
			type A2077 = A2076[];
			type A2078 = A2077[];
			type A2079 = A2078[];
			type A2080 = A2079[];
			type A2081 = A2080[];
			type A2082 = A2081[];
			type A2083 = A2082[];
			type A2084 = A2083[];
			type A2085 = A2084[];
			type A2086 = A2085[];
			type A2087 = A2086[];
			type A2088 = A2087[];
			type A2089 = A2088[];
			type A2090 = A2089[];
			type A2091 = A2090[];
			type A2092 = A2091[];
			type A2093 = A2092[];
			type A2094 = A2093[];
			type A2095 = A2094[];
			type A2096 = A2095[];
			type A2097 = A2096[];
			type A2098 = A2097[];
			type A2099 = A2098[];
			type A2100 = A2099[];
			type A2101 = A2100[];
			type A2102 = A2101[];
			type A2103 = A2102[];
			type A2104 = A2103[];
			type A2105 = A2104[];
			type A2106 = A2105[];
			type A2107 = A2106[];
			type A2108 = A2107[];
			type A2109 = A2108[];
			type A2110 = A2109[];
			type A2111 = A2110[];
			type A2112 = A2111[];
			type A2113 = A2112[];
			type A2114 = A2113[];
			type A2115 = A2114[];
			type A2116 = A2115[];
			type A2117 = A2116[];
			type A2118 = A2117[];
			type A2119 = A2118[];
			type A2120 = A2119[];
			type A2121 = A2120[];
			type A2122 = A2121[];
			type A2123 = A2122[];
			type A2124 = A2123[];
			type A2125 = A2124[];
			type A2126 = A2125[];
			type A2127 = A2126[];
			type A2128 = A2127[];
			type A2129 = A2128[];
			type A2130 = A2129[];
			type A2131 = A2130[];
			type A2132 = A2131[];
			type A2133 = A2132[];
			type A2134 = A2133[];
			type A2135 = A2134[];
			type A2136 = A2135[];
			type A2137 = A2136[];
			type A2138 = A2137[];
			type A2139 = A2138[];
			type A2140 = A2139[];
			type A2141 = A2140[];
			type A2142 = A2141[];
			type A2143 = A2142[];
			type A2144 = A2143[];
			type A2145 = A2144[];
			type A2146 = A2145[];
			type A2147 = A2146[];
			type A2148 = A2147[];
			type A2149 = A2148[];
			type A2150 = A2149[];
			type A2151 = A2150[];
			type A2152 = A2151[];
			type A2153 = A2152[];
			type A2154 = A2153[];
			type A2155 = A2154[];
			type A2156 = A2155[];
			type A2157 = A2156[];
			type A2158 = A2157[];
			type A2159 = A2158[];
			type A2160 = A2159[];
			type A2161 = A2160[];
			type A2162 = A2161[];
			type A2163 = A2162[];
			type A2164 = A2163[];
			type A2165 = A2164[];
			type A2166 = A2165[];
			type A2167 = A2166[];
			type A2168 = A2167[];
			type A2169 = A2168[];
			type A2170 = A2169[];
			type A2171 = A2170[];
			type A2172 = A2171[];
			type A2173 = A2172[];
			type A2174 = A2173[];
			type A2175 = A2174[];
			type A2176 = A2175[];
			type A2177 = A2176[];
			type A2178 = A2177[];
			type A2179 = A2178[];
			type A2180 = A2179[];
			type A2181 = A2180[];
			type A2182 = A2181[];
			type A2183 = A2182[];
			type A2184 = A2183[];
			type A2185 = A2184[];
			type A2186 = A2185[];
			type A2187 = A2186[];
			type A2188 = A2187[];
			type A2189 = A2188[];
			type A2190 = A2189[];
			type A2191 = A2190[];
			type A2192 = A2191[];
			type A2193 = A2192[];
			type A2194 = A2193[];
			type A2195 = A2194[];
			type A2196 = A2195[];
			type A2197 = A2196[];
			type A2198 = A2197[];
			type A2199 = A2198[];
			type A2200 = A2199[];
			type A2201 = A2200[];
			type A2202 = A2201[];
			type A2203 = A2202[];
			type A2204 = A2203[];
			type A2205 = A2204[];
			type A2206 = A2205[];
			type A2207 = A2206[];
			type A2208 = A2207[];
			type A2209 = A2208[];
			type A2210 = A2209[];
			type A2211 = A2210[];
			type A2212 = A2211[];
			type A2213 = A2212[];
			type A2214 = A2213[];
			type A2215 = A2214[];
			type A2216 = A2215[];
			type A2217 = A2216[];
			type A2218 = A2217[];
			type A2219 = A2218[];
			type A2220 = A2219[];
			type A2221 = A2220[];
			type A2222 = A2221[];
			type A2223 = A2222[];
			type A2224 = A2223[];
			type A2225 = A2224[];
			type A2226 = A2225[];
			type A2227 = A2226[];
			type A2228 = A2227[];
			type A2229 = A2228[];
			type A2230 = A2229[];
			type A2231 = A2230[];
			type A2232 = A2231[];
			type A2233 = A2232[];
			type A2234 = A2233[];
			type A2235 = A2234[];
			type A2236 = A2235[];
			type A2237 = A2236[];
			type A2238 = A2237[];
			type A2239 = A2238[];
			type A2240 = A2239[];
			type A2241 = A2240[];
			type A2242 = A2241[];
			type A2243 = A2242[];
			type A2244 = A2243[];
			type A2245 = A2244[];
			type A2246 = A2245[];
			type A2247 = A2246[];
			type A2248 = A2247[];
			type A2249 = A2248[];
			type A2250 = A2249[];
			type A2251 = A2250[];
			type A2252 = A2251[];
			type A2253 = A2252[];
			type A2254 = A2253[];
			type A2255 = A2254[];
			type A2256 = A2255[];
			type A2257 = A2256[];
			type A2258 = A2257[];
			type A2259 = A2258[];
			type A2260 = A2259[];
			type A2261 = A2260[];
			type A2262 = A2261[];
			type A2263 = A2262[];
			type A2264 = A2263[];
			type A2265 = A2264[];
			type A2266 = A2265[];
			type A2267 = A2266[];
			type A2268 = A2267[];
			type A2269 = A2268[];
			type A2270 = A2269[];
			type A2271 = A2270[];
			type A2272 = A2271[];
			type A2273 = A2272[];
			type A2274 = A2273[];
			type A2275 = A2274[];
			type A2276 = A2275[];
			type A2277 = A2276[];
			type A2278 = A2277[];
			type A2279 = A2278[];
			type A2280 = A2279[];
			type A2281 = A2280[];
			type A2282 = A2281[];
			type A2283 = A2282[];
			type A2284 = A2283[];
			type A2285 = A2284[];
			type A2286 = A2285[];
			type A2287 = A2286[];
			type A2288 = A2287[];
			type A2289 = A2288[];
			type A2290 = A2289[];
			type A2291 = A2290[];
			type A2292 = A2291[];
			type A2293 = A2292[];
			type A2294 = A2293[];
			type A2295 = A2294[];
			type A2296 = A2295[];
			type A2297 = A2296[];
			type A2298 = A2297[];
			type A2299 = A2298[];
			type A2300 = A2299[];
			type A2301 = A2300[];
			type A2302 = A2301[];
			type A2303 = A2302[];
			type A2304 = A2303[];
			type A2305 = A2304[];
			type A2306 = A2305[];
			type A2307 = A2306[];
			type A2308 = A2307[];
			type A2309 = A2308[];
			type A2310 = A2309[];
			type A2311 = A2310[];
			type A2312 = A2311[];
			type A2313 = A2312[];
			type A2314 = A2313[];
			type A2315 = A2314[];
			type A2316 = A2315[];
			type A2317 = A2316[];
			type A2318 = A2317[];
			type A2319 = A2318[];
			type A2320 = A2319[];
			type A2321 = A2320[];
			type A2322 = A2321[];
			type A2323 = A2322[];
			type A2324 = A2323[];
			type A2325 = A2324[];
			type A2326 = A2325[];
			type A2327 = A2326[];
			type A2328 = A2327[];
			type A2329 = A2328[];
			type A2330 = A2329[];
			type A2331 = A2330[];
			type A2332 = A2331[];
			type A2333 = A2332[];
			type A2334 = A2333[];
			type A2335 = A2334[];
			type A2336 = A2335[];
			type A2337 = A2336[];
			type A2338 = A2337[];
			type A2339 = A2338[];
			type A2340 = A2339[];
			type A2341 = A2340[];
			type A2342 = A2341[];
			type A2343 = A2342[];
			type A2344 = A2343[];
			type A2345 = A2344[];
			type A2346 = A2345[];
			type A2347 = A2346[];
			type A2348 = A2347[];
			type A2349 = A2348[];
			type A2350 = A2349[];
			type A2351 = A2350[];
			type A2352 = A2351[];
			type A2353 = A2352[];
			type A2354 = A2353[];
			type A2355 = A2354[];
			type A2356 = A2355[];
			type A2357 = A2356[];
			type A2358 = A2357[];
			type A2359 = A2358[];
			type A2360 = A2359[];
			type A2361 = A2360[];
			type A2362 = A2361[];
			type A2363 = A2362[];
			type A2364 = A2363[];
			type A2365 = A2364[];
			type A2366 = A2365[];
			type A2367 = A2366[];
			type A2368 = A2367[];
			type A2369 = A2368[];
			type A2370 = A2369[];
			type A2371 = A2370[];
			type A2372 = A2371[];
			type A2373 = A2372[];
			type A2374 = A2373[];
			type A2375 = A2374[];
			type A2376 = A2375[];
			type A2377 = A2376[];
			type A2378 = A2377[];
			type A2379 = A2378[];
			type A2380 = A2379[];
			type A2381 = A2380[];
			type A2382 = A2381[];
			type A2383 = A2382[];
			type A2384 = A2383[];
			type A2385 = A2384[];
			type A2386 = A2385[];
			type A2387 = A2386[];
			type A2388 = A2387[];
			type A2389 = A2388[];
			type A2390 = A2389[];
			type A2391 = A2390[];
			type A2392 = A2391[];
			type A2393 = A2392[];
			type A2394 = A2393[];
			type A2395 = A2394[];
			type A2396 = A2395[];
			type A2397 = A2396[];
			type A2398 = A2397[];
			type A2399 = A2398[];
			type A2400 = A2399[];
			type A2401 = A2400[];
			type A2402 = A2401[];
			type A2403 = A2402[];
			type A2404 = A2403[];
			type A2405 = A2404[];
			type A2406 = A2405[];
			type A2407 = A2406[];
			type A2408 = A2407[];
			type A2409 = A2408[];
			type A2410 = A2409[];
			type A2411 = A2410[];
			type A2412 = A2411[];
			type A2413 = A2412[];
			type A2414 = A2413[];
			type A2415 = A2414[];
			type A2416 = A2415[];
			type A2417 = A2416[];
			type A2418 = A2417[];
			type A2419 = A2418[];
			type A2420 = A2419[];
			type A2421 = A2420[];
			type A2422 = A2421[];
			type A2423 = A2422[];
			type A2424 = A2423[];
			type A2425 = A2424[];
			type A2426 = A2425[];
			type A2427 = A2426[];
			type A2428 = A2427[];
			type A2429 = A2428[];
			type A2430 = A2429[];
			type A2431 = A2430[];
			type A2432 = A2431[];
			type A2433 = A2432[];
			type A2434 = A2433[];
			type A2435 = A2434[];
			type A2436 = A2435[];
			type A2437 = A2436[];
			type A2438 = A2437[];
			type A2439 = A2438[];
			type A2440 = A2439[];
			type A2441 = A2440[];
			type A2442 = A2441[];
			type A2443 = A2442[];
			type A2444 = A2443[];
			type A2445 = A2444[];
			type A2446 = A2445[];
			type A2447 = A2446[];
			type A2448 = A2447[];
			type A2449 = A2448[];
			type A2450 = A2449[];
			type A2451 = A2450[];
			type A2452 = A2451[];
			type A2453 = A2452[];
			type A2454 = A2453[];
			type A2455 = A2454[];
			type A2456 = A2455[];
			type A2457 = A2456[];
			type A2458 = A2457[];
			type A2459 = A2458[];
			type A2460 = A2459[];
			type A2461 = A2460[];
			type A2462 = A2461[];
			type A2463 = A2462[];
			type A2464 = A2463[];
			type A2465 = A2464[];
			type A2466 = A2465[];
			type A2467 = A2466[];
			type A2468 = A2467[];
			type A2469 = A2468[];
			type A2470 = A2469[];
			type A2471 = A2470[];
			type A2472 = A2471[];
			type A2473 = A2472[];
			type A2474 = A2473[];
			type A2475 = A2474[];
			type A2476 = A2475[];
			type A2477 = A2476[];
			type A2478 = A2477[];
			type A2479 = A2478[];
			type A2480 = A2479[];
			type A2481 = A2480[];
			type A2482 = A2481[];
			type A2483 = A2482[];
			type A2484 = A2483[];
			type A2485 = A2484[];
			type A2486 = A2485[];
			type A2487 = A2486[];
			type A2488 = A2487[];
			type A2489 = A2488[];
			type A2490 = A2489[];
			type A2491 = A2490[];
			type A2492 = A2491[];
			type A2493 = A2492[];
			type A2494 = A2493[];
			type A2495 = A2494[];
			type A2496 = A2495[];
			type A2497 = A2496[];
			type A2498 = A2497[];
			type A2499 = A2498[];
			type A2500 = A2499[];
			interface Base<V> {
				[key: string]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string]: A2500;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("lints a 150,000-element array literal without exceeding the argument limit while recording nested names", () => {
		const elements = Array.from({ length: 150_000 }, () => "0").join(", ");
		const diagnostics = lintAntiSlopFixture(`const data = [${elements}];\nvoid data;`, [
			"no-unsafe-dictionary-type",
		]);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBe(0);
	});

	it("keeps an inherited unsafe symbol key when the override covers a different key union", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string | symbol]: V;
			}
			interface Derived extends Base<unknown> {
				[key: string | number]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("keeps an inherited unsafe signature when the override key is an alias of a different key type", () => {
		const diagnostics = lintAntiSlopFixture(
			`type SymbolKey = symbol;
			type StringKey = string;
			interface Base<V> {
				[key: SymbolKey]: V;
			}
			interface Derived extends Base<unknown> {
				[key: StringKey]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("keeps an inherited unsafe mapped signature whose keys an as clause remaps to another key type", () => {
		const diagnostics = lintAntiSlopFixture(
			`type Base<V> = { [K in string as symbol]: V };
			interface Derived extends Base<unknown> {
				[key: string]: string;
			}
			const derived: Derived = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});

	it("keeps an inherited unsafe signature when the override value is a conditional the rule cannot classify", () => {
		const diagnostics = lintAntiSlopFixture(
			`interface Base<V> {
				[key: string]: V;
			}
			interface Derived<T> extends Base<unknown> {
				[key: string]: T extends string ? string : unknown;
			}
			const derived: Derived<number> = {};
			void derived;`,
			["no-unsafe-dictionary-type"],
		);

		expect(diagnosticCount(diagnostics, "no-unsafe-dictionary-type")).toBeGreaterThan(0);
	});
});
