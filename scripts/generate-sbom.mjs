#!/usr/bin/env node
/**
 * SBOM generator (release-distribution.md §3.5 / phase 09 §2.8).
 *
 * Emits a CycloneDX 1.5 JSON bill of materials plus a Markdown summary from
 * the two lockfiles' resolved graphs:
 *   - Rust: `cargo metadata --locked` (every crate in Cargo.lock, with its
 *     declared SPDX license expression and registry source)
 *   - npm:  `pnpm licenses list --json` for the desktop app, with the
 *     production set from `pnpm list --prod` so dev-only tooling is marked.
 *
 * The output is evidence, not a claim of legal review: licenses are the
 * expressions the packages declare. cargo-deny (deny.toml) is the gate that
 * fails on anything outside the allow list.
 *
 * Usage: node scripts/generate-sbom.mjs [out-dir]   (default docs/release/sbom)
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const desktopDir = join(repoRoot, "apps", "desktop");
const outDir = resolve(process.argv[2] ?? join(repoRoot, "docs", "release", "sbom"));

function run(command, args, cwd) {
  return execFileSync(command, args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
}

// ---- Rust -------------------------------------------------------------------

const cargoMetadata = JSON.parse(
  run("cargo", ["metadata", "--format-version", "1", "--locked"], repoRoot)
);
const workspaceIds = new Set(cargoMetadata.workspace_members);
const rustComponents = cargoMetadata.packages
  .filter((pkg) => !workspaceIds.has(pkg.id))
  .map((pkg) => ({
    type: "library",
    "bom-ref": `pkg:cargo/${pkg.name}@${pkg.version}`,
    name: pkg.name,
    version: pkg.version,
    purl: `pkg:cargo/${pkg.name}@${pkg.version}`,
    licenses: pkg.license
      ? [{ expression: pkg.license }]
      : [{ license: { name: pkg.license_file ? "see license file" : "UNKNOWN" } }],
    externalReferences: pkg.repository
      ? [{ type: "vcs", url: pkg.repository }]
      : [],
    properties: [
      { name: "ecosystem", value: "cargo" },
      { name: "source", value: pkg.source ?? "workspace" },
    ],
  }))
  .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

// ---- npm --------------------------------------------------------------------

const prodTree = JSON.parse(
  run("pnpm", ["list", "--prod", "--depth", "Infinity", "--json"], desktopDir)
);
const prodKeys = new Set();
function walk(node) {
  for (const group of ["dependencies", "optionalDependencies"]) {
    for (const [name, info] of Object.entries(node?.[group] ?? {})) {
      prodKeys.add(`${name}@${info.version}`);
      walk(info);
    }
  }
}
for (const project of prodTree) walk(project);

const licensesByName = JSON.parse(run("pnpm", ["licenses", "list", "--json"], desktopDir));
const npmComponents = [];
for (const [license, entries] of Object.entries(licensesByName)) {
  for (const entry of entries) {
    for (const version of entry.versions) {
      const key = `${entry.name}@${version}`;
      npmComponents.push({
        type: "library",
        "bom-ref": `pkg:npm/${entry.name}@${version}`,
        name: entry.name,
        version,
        purl: `pkg:npm/${encodeURIComponent(entry.name).replace("%40", "@")}@${version}`,
        licenses: [{ expression: license }],
        externalReferences: entry.homepage ? [{ type: "website", url: entry.homepage }] : [],
        properties: [
          { name: "ecosystem", value: "npm" },
          { name: "scope", value: prodKeys.has(key) ? "runtime" : "development" },
        ],
      });
    }
  }
}
npmComponents.sort(
  (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version)
);

// ---- Emit -------------------------------------------------------------------

const appVersion = cargoMetadata.packages.find((pkg) => pkg.name === "app")?.version ?? "0.0.0";
const bom = {
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  serialNumber: `urn:uuid:${randomUUID()}`,
  version: 1,
  metadata: {
    timestamp: new Date().toISOString(),
    tools: [{ vendor: "hostlume", name: "generate-sbom.mjs", version: "1" }],
    component: {
      type: "application",
      "bom-ref": `pkg:generic/app@${appVersion}`,
      name: "app",
      version: appVersion,
      licenses: [{ expression: "GPL-3.0-only" }],
    },
  },
  components: [...rustComponents, ...npmComponents],
};

mkdirSync(outDir, { recursive: true });
const jsonPath = join(outDir, `sbom-${appVersion}.cdx.json`);
writeFileSync(jsonPath, `${JSON.stringify(bom, null, 2)}\n`);

function tally(components) {
  const counts = new Map();
  for (const component of components) {
    const expression = component.licenses[0]?.expression ?? component.licenses[0]?.license?.name ?? "UNKNOWN";
    counts.set(expression, (counts.get(expression) ?? 0) + 1);
  }
  return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}

const runtimeNpm = npmComponents.filter(
  (component) => component.properties.some((p) => p.name === "scope" && p.value === "runtime")
);
const lines = [
  `# SBOM summary — app ${appVersion}`,
  "",
  `Generated ${bom.metadata.timestamp} by \`scripts/generate-sbom.mjs\` from \`Cargo.lock\` and \`apps/desktop/pnpm-lock.yaml\`.`,
  `Machine-readable: [\`sbom-${appVersion}.cdx.json\`](./sbom-${appVersion}.cdx.json) (CycloneDX 1.5).`,
  "",
  "Licenses below are the expressions the packages declare. The enforcement gate is `cargo deny check` (deny.toml allow list) and `pnpm audit`; see docs/evidence/DEPENDENCY_LICENSE_SCAN.md for the human review.",
  "",
  `## Rust crates (${rustComponents.length} resolved, all targets in Cargo.lock)`,
  "",
  "| License expression | Crates |",
  "|---|---|",
  ...tally(rustComponents).map(([expression, count]) => `| ${expression} | ${count} |`),
  "",
  `## npm packages (${npmComponents.length} installed; ${runtimeNpm.length} in the shipped runtime bundle, the rest build/test tooling)`,
  "",
  "| License expression | Packages (runtime) | Packages (all) |",
  "|---|---|---|",
  ...tally(npmComponents).map(([expression, count]) => {
    const runtimeCount = runtimeNpm.filter(
      (component) => (component.licenses[0]?.expression ?? "") === expression
    ).length;
    return `| ${expression} | ${runtimeCount} | ${count} |`;
  }),
  "",
  "## Runtime npm packages",
  "",
  "| Package | Version | License |",
  "|---|---|---|",
  ...runtimeNpm.map(
    (component) => `| ${component.name} | ${component.version} | ${component.licenses[0]?.expression ?? ""} |`
  ),
  "",
];
const mdPath = join(outDir, "README.md");
writeFileSync(mdPath, `${lines.join("\n")}\n`);

console.log(`wrote ${jsonPath}`);
console.log(`wrote ${mdPath}`);
console.log(`rust=${rustComponents.length} npm=${npmComponents.length} npm-runtime=${runtimeNpm.length}`);
