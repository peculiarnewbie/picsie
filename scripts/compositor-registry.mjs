// Project documentation tooling only. Never editor/runtime behavior.
import { readFile, writeFile, access } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const registry = JSON.parse(await readFile(resolve(root, 'docs/compositor-registry.json'), 'utf8'));
const implementation = new Set(['implemented', 'partial', 'missing', 'adapted']);
const polish = new Set(['verified', 'partial', 'missing', 'adapted', 'unverified']);
const verification = new Set(['native', 'tests', 'source', 'unverified']);
const ids = new Set();
const index = new Map(registry.sourceIndex.map((entry) => [entry.path, entry]));
const sourceRoot = process.env.COMPOSITOR_REFERENCE_PATH;
const escape = (text) => text.replaceAll('|', '\\|').replaceAll('\n', ' ');
const sourceLink = (path) => `[${path.split('/').at(-1)}](${registry.reference.repository}/blob/${registry.reference.revision}/${path})`;
const localLink = (path) => `[${path.split('/').at(-1)}](../${path})`;
const evidenceLink = (path) => index.has(path) ? sourceLink(path) : localLink(path);
const nativeVerifier = await readFile(resolve(root, 'crates/picsie-desktop/verify.py'), 'utf8');
const assert = (condition, message) => { if (!condition) throw new Error(message); };
assert(registry.schemaVersion === 1, 'Unsupported registry schema');
assert(registry.reference.revision === '609dbeae2ef68ef4fc82d67e4981a49852eb6e13', 'Reference revision changed without a source review');
assert(index.size === registry.sourceIndex.length, 'Duplicate source-index path');
for (const entry of registry.sourceIndex) {
  assert(/^[a-f0-9]{64}$/.test(entry.sha256), `Invalid source digest: ${entry.path}`);
  assert(/^(Compositor|CompositorTests)\//.test(entry.path) && !entry.path.includes('..'), `Invalid source path: ${entry.path}`);
  if (sourceRoot) {
    const data = await readFile(resolve(sourceRoot, entry.path));
    assert(createHash('sha256').update(data).digest('hex') === entry.sha256, `Upstream source drift: ${entry.path}`);
  }
}
const all = registry.areas.flatMap((area) => area.items);
for (const area of registry.areas) {
  assert(area.upstream.length > 0 && area.local.length > 0, `Missing source map: ${area.id}`);
  for (const path of area.upstream) assert(index.has(path), `Unindexed upstream source: ${path}`);
  for (const path of area.local) await access(resolve(root, path));
  for (const item of area.items) {
    assert(item.id.startsWith(`${area.id}.`) && !ids.has(item.id), `Invalid or duplicate ID: ${item.id}`);
    ids.add(item.id);
    assert(implementation.has(item.implementation) && polish.has(item.polish) && verification.has(item.verification), `Invalid status: ${item.id}`);
    assert(item.behavior && item.gap && Array.isArray(item.evidence), `Incomplete row: ${item.id}`);
    assert(item.polish !== 'verified' || (item.verification === 'native' && item.evidence.length > 0), `Polish needs native evidence: ${item.id}`);
    assert(item.verification === 'unverified' || item.evidence.length > 0, `Verification needs evidence: ${item.id}`);
    for (const path of item.upstream ?? []) assert(index.has(path), `Unindexed row source: ${item.id}: ${path}`);
    if (item.verification === 'native') assert(item.nativeChecks?.length > 0, `Native evidence needs named checks: ${item.id}`);
    for (const check of item.nativeChecks ?? []) assert(nativeVerifier.includes(check), `Native check missing: ${item.id}: ${check}`);
    for (const path of item.evidence) if (!index.has(path)) await access(resolve(root, path));
  }
}
const tally = (field) => [...new Set(all.map((x) => x[field]))].sort().map((status) => `${all.filter((x) => x[field] === status).length} ${status}`).join(', ');
const lines = [
  '# Compositor feature and polish registry', '',
  'Generated from [compositor-registry.json](compositor-registry.json). Edit the JSON, then run `npm run registry:update`; `npm run check:registry` rejects stale output, invalid statuses, missing evidence and broken source references.', '',
  `Reference: [Compositor ${registry.reference.revision.slice(0, 8)}](${registry.reference.repository}/tree/${registry.reference.revision}), ${registry.reference.license}. Reviewed ${registry.reviewedOn}.`, '',
  registry.scope, '',
  `**${all.length} behavior entries**, **${registry.sourceIndex.length} indexed code/test files**, **${registry.sourceIndex.reduce((n, x) => n + x.fixtures.length, 0)} upstream fixture names**.`, '',
  `Implementation: ${tally('implementation')}. Polish: ${tally('polish')}. Verification: ${tally('verification')}.`, '',
  'Implementation and polish are independent. **Implemented** means the capability exists, **partial** means an explicit behavior gap remains, **adapted** records an intentional platform/user choice, and **missing** means absent. **Verified polish** requires real native interaction/screenshot evidence for that row; tests or source inspection alone never qualify. Native verification here means the stated Linux environment, not macOS runtime equivalence. “Gap” also records verification limits or intentional differences.', '',
  'When changing a feature, update its stable ID, status, gap and evidence. Add rows for newly discovered behavior; do not delete missing rows or turn prototype deviations into parity claims. Add exact regression evidence after it passes, and keep platform limits open until tested.', '',
  '## Areas', '', ...registry.areas.map((a) => `- [${a.title}](#${a.id})`), '',
];
for (const area of registry.areas) {
  lines.push(`<a id="${area.id}"></a>`, `## ${area.title}`, '', `Pinned source: ${area.upstream.map(sourceLink).join(', ')}.`, '', `Local: ${area.local.map(localLink).join(', ')}.`, '',
    '| ID | Upstream behavior / expected polish | Implementation | Polish | Verification | Gap / adaptation / evidence |',
    '| --- | --- | --- | --- | --- | --- |');
  for (const item of area.items) {
    const evidence = item.evidence.length ? ` Evidence: ${item.evidence.map(evidenceLink).join(', ')}.` : '';
    const source = item.upstream?.length ? ` Source: ${item.upstream.map(sourceLink).join(', ')}.` : '';
    const checks = item.nativeChecks?.length ? ` Checks: ${item.nativeChecks.map(escape).join('; ')}.` : '';
    lines.push(`| ${item.id} | ${escape(item.behavior)} | ${item.implementation} | ${item.polish} | ${item.verification} | ${escape(item.gap)}${source}${evidence}${checks} |`);
  }
  lines.push('');
}
lines.push('## Complete pinned source and test index', '',
  'The machine-readable registry includes SHA-256 digests and symbol names for every Swift/C/header file under the pinned app and test directories. All fixture names are inventoried below; these are upstream scenarios to port or review, not a claim that the Swift suite ran here. The area tables are the reviewed feature-level checklist; indexed symbols can reveal finer behaviors that need new rows.', '',
  '| Pinned module | Indexed symbols | Upstream fixtures (execution unverified) |', '| --- | --- | --- |');
for (const entry of registry.sourceIndex) lines.push(`| ${sourceLink(entry.path)} | ${entry.symbols.length} | ${entry.fixtures.map((x) => `\`${x}\``).join(', ') || '—'} |`);
const output = `${lines.join('\n')}\n`;
const destination = resolve(root, 'docs/compositor-registry.md');
if (process.argv.includes('--check')) assert(await readFile(destination, 'utf8') === output, 'Registry Markdown is stale. Run npm run registry:update.');
else await writeFile(destination, output);
console.log(`Compositor registry: ${all.length} behavior entries validated${process.argv.includes('--check') ? '' : ' and documentation updated'}.`);
