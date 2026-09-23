#!/usr/bin/env node
// @ts-check
/**
 * Validate every starter template in moddin-agent/templates/ against the
 * capability JSON Schema. Exits non-zero if any template fails.
 *
 * Used in CI alongside the Rust tests so a template cannot drift away
 * from the runner contract.
 */
import { readFile, readdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parse as parseYaml } from "yaml";
import Ajv from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const SCHEMA_PATH = join(ROOT, "schema", "capability.schema.json");
const TEMPLATES_DIR = join(ROOT, "templates");

const schema = JSON.parse(await readFile(SCHEMA_PATH, "utf8"));
const ajv = new Ajv({ allErrors: true, strict: false });
addFormats(ajv);
const validate = ajv.compile(schema);

const files = (await readdir(TEMPLATES_DIR)).filter((f) => /\.ya?ml$/i.test(f));
let failures = 0;

for (const file of files) {
  const raw = await readFile(join(TEMPLATES_DIR, file), "utf8");
  const spec = parseYaml(raw);
  const ok = validate(spec);
  if (!ok) {
    failures += 1;
    console.error(`✗ ${file}`);
    for (const err of validate.errors ?? []) {
      console.error(`    ${err.instancePath} ${err.message}`);
    }
  } else {
    console.log(`✓ ${file}`);
  }
}

if (failures > 0) {
  console.error(`\n${failures} template(s) failed validation.`);
  process.exit(1);
} else {
  console.log(`\nAll ${files.length} templates valid.`);
}