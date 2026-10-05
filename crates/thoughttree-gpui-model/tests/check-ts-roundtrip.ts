// Run by the Rust integration test; no npm modules or providers are required.
import assert from 'node:assert/strict';
import { GraphSerialize } from '../../../packages/graph-model/src/serialize';
const [original, saved] = await Promise.all(Bun.argv.slice(2).map(path => Bun.file(path).json()));
assert.deepEqual(GraphSerialize.toJSON(GraphSerialize.fromJSON(saved.graph)), GraphSerialize.toJSON(GraphSerialize.fromJSON(original.graph)));
assert.deepEqual(saved.projectModelPreferences, original.projectModelPreferences);
assert.deepEqual(saved.projectEffortPreferences, original.projectEffortPreferences);
