#!/usr/bin/env node
// @ts-check
/**
 * moddin-agent — MCP server.
 *
 * Lets an AI assistant add new mods to Moddin Desktop by authoring
 * capability recipes the user then previews and applies inside the app.
 *
 * Transport: MCP stdio.
 * The server is *purely an authoring assistant*. It never touches game
 * files, registry keys, or running processes. The only filesystem write
 * it performs is into %LOCALAPPDATA%/Moddin/capabilities/, which Moddin
 * Desktop itself scans on launch.
 *
 * Source of truth: docs/CAPABILITY-CONTRACT.md, src-tauri/src/capability.rs
 * in the parent Moddin project.
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { readFile, writeFile, readdir, mkdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join, resolve, basename, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";
import Ajv from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import { z } from "zod";

// ---------------------------------------------------------------------------
// Paths and configuration
// ---------------------------------------------------------------------------

const __dirname = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(__dirname, "..");

const PROJECT_ROOT =
  process.env.MODDIN_PROJECT_ROOT ??
  resolve(PACKAGE_ROOT, ".."); // default: parent of moddin-agent/

const BUILT_IN_CAPABILITIES_DIR = join(PROJECT_ROOT, "src-tauri", "capabilities");
const GAMES_CATALOG_DIR = join(PROJECT_ROOT, "src", "catalog", "games");
const SCHEMA_PATH = join(PACKAGE_ROOT, "schema", "capability.schema.json");
const TEMPLATES_DIR = join(PACKAGE_ROOT, "templates");

const LOCAL_CAPABILITIES_DIR =
  process.env.MODDIN_LOCAL_CAPABILITIES_DIR ||
  (process.env.LOCALAPPDATA
    ? join(process.env.LOCALAPPDATA, "Moddin", "capabilities")
    : null);

// ---------------------------------------------------------------------------
// Constants mirrored from src-tauri/src/capability.rs / builtin_*.rs
// ---------------------------------------------------------------------------

const STEP_KINDS = Object.freeze({
  "extract-zip": {
    summary: "Extract a zip archive into the game's executable directory.",
    fields: ["archivePathField", "archiveBytesField"],
    touches: ["executableDir"],
  },
  "verify-hash": {
    summary: "Read a file and compare its SHA-256 to an expected value.",
    fields: ["pathField", "expectedField"],
    touches: ["executableDir"],
  },
  "file-delete": {
    summary: "Delete a single file resolved against a config field.",
    fields: ["pathField"],
    touches: ["executableDir"],
  },
  "write-text-file": {
    summary: "Write a text file with `{name}` placeholders rendered from config.",
    fields: ["pathField", "template"],
    touches: ["executableDir"],
  },
  "write-binary-file": {
    summary: "Write a base64-decoded binary payload to a path field.",
    fields: ["pathField", "base64"],
    touches: ["executableDir"],
  },
  "move-file": {
    summary: "Rename / move a file inside the executable directory.",
    fields: ["fromField", "toField"],
    touches: ["executableDir"],
  },
  "spawn-process": {
    summary: "Start an executable (resolved against the game dir).",
    fields: ["executable", "executableField"],
    touches: ["process"],
  },
  "kill-process": {
    summary: "taskkill /IM <name> /T (optionally /F) on a configured name.",
    fields: ["processName", "processNameField", "force"],
    touches: ["process"],
  },
  "registry-write": {
    summary: "reg.exe add <key> /v <name> /t <type> /d <data> /f. HKCU only.",
    fields: ["keyField", "nameField", "typeField", "dataField"],
    touches: ["registry"],
  },
  "registry-delete": {
    summary: "reg.exe delete <key> /v <name> /f. HKCU only.",
    fields: ["keyField", "nameField"],
    touches: ["registry"],
  },
});

const CHECK_KINDS = Object.freeze({
  "process-running": {
    summary: "Returns true when the named process is alive.",
    fields: ["processName", "processNameField"],
    severity: "warning",
  },
  "file-exists": {
    summary: "Returns true when pathField/path resolves to an existing file.",
    fields: ["path", "pathField"],
    severity: "warning",
  },
  "file-absent": {
    summary: "Returns true when the resolved path is missing.",
    fields: ["path", "pathField"],
    severity: "warning",
  },
  "archive-reachable": {
    summary: "HTTPS HEAD against the configured URL.",
    fields: ["url", "urlField"],
    severity: "warning",
  },
  "archive-sha256": {
    summary: "Downloads the archive, computes SHA-256, compares to expected.",
    fields: ["url", "urlField", "expected", "expectedField"],
    severity: "blocker",
  },
});

// ---------------------------------------------------------------------------
// Schema validation
// ---------------------------------------------------------------------------

async function loadSchema() {
  const raw = await readFile(SCHEMA_PATH, "utf8");
  return JSON.parse(raw);
}

function makeAjv() {
  const ajv = new Ajv({ allErrors: true, strict: false });
  addFormats(ajv);
  return ajv;
}

async function validateSpec(spec) {
  const schema = await loadSchema();
  const ajv = makeAjv();
  const validate = ajv.compile(schema);
  const ok = validate(spec);
  return {
    ok,
    errors: ok ? [] : (validate.errors ?? []).map(formatAjvError),
  };
}

function formatAjvError(err) {
  const at = err.instancePath || "(root)";
  return `${at} ${err.message ?? "invalid"}`;
}

// ---------------------------------------------------------------------------
// Catalog readers
// ---------------------------------------------------------------------------

async function readGameCatalog() {
  if (!existsSync(GAMES_CATALOG_DIR)) return [];
  const files = await readdir(GAMES_CATALOG_DIR);
  const games = [];
  for (const file of files) {
    if (!/\.ya?ml$/i.test(file)) continue;
    try {
      const raw = await readFile(join(GAMES_CATALOG_DIR, file), "utf8");
      const parsed = parseYaml(raw);
      if (!parsed?.id) continue;
      games.push({
        id: parsed.id,
        name: parsed.name ?? parsed.id,
        steamAppId: parsed.steamAppId ?? null,
        executable: parsed.executable ?? null,
        pcgwSlug: parsed.pcgwSlug ?? null,
        modules: Array.isArray(parsed.modules)
          ? parsed.modules.map((m) => ({
              id: m.id,
              name: m.name,
              category: m.category,
              status: m.status,
            }))
          : [],
      });
    } catch (err) {
      // Skip malformed game catalog entries but keep going.
    }
  }
  return games;
}

async function readBuiltInCapabilities() {
  if (!existsSync(BUILT_IN_CAPABILITIES_DIR)) return [];
  const files = await readdir(BUILT_IN_CAPABILITIES_DIR);
  const out = [];
  for (const file of files) {
    if (!/\.ya?ml$/i.test(file)) continue;
    try {
      const raw = await readFile(join(BUILT_IN_CAPABILITIES_DIR, file), "utf8");
      const parsed = parseYaml(raw);
      if (!parsed?.id) continue;
      out.push({
        id: parsed.id,
        displayName: parsed.displayName ?? parsed.id,
        category: parsed.category ?? null,
        status: parsed.status ?? null,
        origin: "builtIn",
        path: join(BUILT_IN_CAPABILITIES_DIR, file),
      });
    } catch (err) {
      // skip
    }
  }
  return out;
}

async function readLocalCapabilities() {
  if (!LOCAL_CAPABILITIES_DIR || !existsSync(LOCAL_CAPABILITIES_DIR)) return [];
  const files = await readdir(LOCAL_CAPABILITIES_DIR);
  const out = [];
  for (const file of files) {
    if (!/\.ya?ml$/i.test(file)) continue;
    try {
      const raw = await readFile(join(LOCAL_CAPABILITIES_DIR, file), "utf8");
      const parsed = parseYaml(raw);
      if (!parsed?.id) continue;
      out.push({
        id: parsed.id,
        displayName: parsed.displayName ?? parsed.id,
        category: parsed.category ?? null,
        status: parsed.status ?? null,
        origin: "local",
        path: join(LOCAL_CAPABILITIES_DIR, file),
      });
    } catch (err) {
      // skip
    }
  }
  return out;
}

async function readCapabilityById(id) {
  const all = [...(await readBuiltInCapabilities()), ...(await readLocalCapabilities())];
  const found = all.find((c) => c.id === id);
  if (!found) return null;
  const raw = await readFile(found.path, "utf8");
  return { meta: found, yaml: raw, parsed: parseYaml(raw) };
}

// ---------------------------------------------------------------------------
// Static dry-run (best-effort, no execution)
// ---------------------------------------------------------------------------

function previewPlan(spec) {
  const lines = [];
  lines.push(`Capability: ${spec.id} (${spec.displayName})`);
  lines.push(`Category: ${spec.category} | Status: ${spec.status}`);
  if (spec.supportedEngines?.length) {
    lines.push(`Supported engines: ${spec.supportedEngines.join(", ")}`);
  }
  if (spec.safetyNotes?.length) {
    lines.push("Safety notes:");
    for (const note of spec.safetyNotes) lines.push(`  - ${note}`);
  }

  if (Array.isArray(spec.configSchema) && spec.configSchema.length) {
    lines.push("");
    lines.push("Config fields the user will be prompted for:");
    for (const f of spec.configSchema) {
      const required = f.required ? " (required)" : "";
      lines.push(`  - ${f.name}: ${f.type}${required}`);
      if (f.description) lines.push(`      ${f.description}`);
    }
  }

  if (Array.isArray(spec.install) && spec.install.length) {
    lines.push("");
    lines.push("Install plan (what Moddin will do, in order):");
    spec.install.forEach((step, i) => {
      lines.push(`  ${i + 1}. ${step.kind} — ${step.description ?? "(no description)"}`);
      const meta = STEP_KINDS[step.kind];
      if (meta?.touches) {
        lines.push(`     touches: ${meta.touches.join(", ")}`);
      }
    });
  } else {
    lines.push("");
    lines.push("(No install steps declared. Moddin will only run checks.)");
  }

  if (Array.isArray(spec.uninstall) && spec.uninstall.length) {
    lines.push("");
    lines.push("Uninstall plan:");
    spec.uninstall.forEach((step, i) => {
      lines.push(`  ${i + 1}. ${step.kind} — ${step.description ?? "(no description)"}`);
    });
  }

  if (Array.isArray(spec.checks) && spec.checks.length) {
    lines.push("");
    lines.push("Pre-flight checks:");
    for (const c of spec.checks) {
      lines.push(`  [${c.severity ?? "info"}] ${c.label} (${c.kind})`);
    }
  }

  if (Array.isArray(spec.verify) && spec.verify.length) {
    lines.push("");
    lines.push("Post-install verify:");
    for (const c of spec.verify) {
      lines.push(`  [${c.severity ?? "info"}] ${c.label} (${c.kind})`);
    }
  }

  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// MCP server setup
// ---------------------------------------------------------------------------

const server = new McpServer({
  name: "moddin",
  version: "0.1.0",
});

// Tool: list_supported_games --------------------------------------------------
server.tool(
  "list_supported_games",
  "List every game Moddin's built-in catalog recognises. Returns id, name, executable relative path, supported engines (where known) and the module list for each.",
  {},
  async () => {
    const games = await readGameCatalog();
    return {
      content: [
        { type: "text", text: JSON.stringify({ count: games.length, games }, null, 2) },
      ],
    };
  }
);

// Tool: get_game_info ---------------------------------------------------------
server.tool(
  "get_game_info",
  "Deep-dive on one game: id, steamAppId, executable relative path, supported modules (with category and status).",
  { gameId: z.string().min(1) },
  async ({ gameId }) => {
    const games = await readGameCatalog();
    const game = games.find((g) => g.id === gameId);
    if (!game) {
      return { isError: true, content: [{ type: "text", text: `Unknown gameId "${gameId}". Call list_supported_games for the catalogue.` }] };
    }
    return { content: [{ type: "text", text: JSON.stringify(game, null, 2) }] };
  }
);

// Tool: list_capabilities -----------------------------------------------------
server.tool(
  "list_capabilities",
  "List every Moddin capability currently shipped (built-in + user-local). Returns id, displayName, category, status, origin.",
  { origin: z.enum(["all", "builtIn", "local"]).default("all") },
  async ({ origin }) => {
    const builtIn = await readBuiltInCapabilities();
    const local = await readLocalCapabilities();
    const merged = origin === "all" ? [...builtIn, ...local] : origin === "builtIn" ? builtIn : local;
    return {
      content: [
        { type: "text", text: JSON.stringify({ count: merged.length, capabilities: merged }, null, 2) },
      ],
    };
  }
);

// Tool: get_capability --------------------------------------------------------
server.tool(
  "get_capability",
  "Read one capability by id. Returns the full YAML text and parsed spec.",
  { capabilityId: z.string().min(1) },
  async ({ capabilityId }) => {
    const cap = await readCapabilityById(capabilityId);
    if (!cap) {
      return { isError: true, content: [{ type: "text", text: `Unknown capabilityId "${capabilityId}".` }] };
    }
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify({ meta: cap.meta, yaml: cap.yaml, parsed: cap.parsed }, null, 2),
        },
      ],
    };
  }
);

// Tool: get_step_kinds --------------------------------------------------------
server.tool(
  "get_step_kinds",
  "Enumerate every install/uninstall step kind the Moddin runner knows about, with the param fields each kind accepts and which area of the system it touches (executableDir / process / registry).",
  {},
  async () => {
    const out = Object.entries(STEP_KINDS).map(([kind, meta]) => ({
      kind,
      summary: meta.summary,
      fields: meta.fields,
      touches: meta.touches,
    }));
    return { content: [{ type: "text", text: JSON.stringify(out, null, 2) }] };
  }
);

// Tool: get_check_kinds -------------------------------------------------------
server.tool(
  "get_check_kinds",
  "Enumerate every check kind (pre-flight + verify) the runner understands, with the param fields each one accepts.",
  {},
  async () => {
    const out = Object.entries(CHECK_KINDS).map(([kind, meta]) => ({
      kind,
      summary: meta.summary,
      fields: meta.fields,
      defaultSeverity: meta.severity,
    }));
    return { content: [{ type: "text", text: JSON.stringify(out, null, 2) }] };
  }
);

// Tool: get_capability_template ---------------------------------------------
server.tool(
  "get_capability_template",
  "Return the starter YAML for one of the four supported patterns: extract-zip, proxy-dll, registry-override, vr-runtime-overlay. The agent fills the <placeholders> and the user's config values.",
  {
    pattern: z.enum(["extract-zip", "proxy-dll", "registry-override", "vr-runtime-overlay"]),
  },
  async ({ pattern }) => {
    const file = join(TEMPLATES_DIR, `${pattern}.yaml`);
    if (!existsSync(file)) {
      return { isError: true, content: [{ type: "text", text: `Template "${pattern}" not found.` }] };
    }
    const raw = await readFile(file, "utf8");
    return { content: [{ type: "text", text: raw }] };
  }
);

// Tool: validate_capability_yaml ---------------------------------------------
server.tool(
  "validate_capability_yaml",
  "Parse + schema-validate a candidate capability YAML. Returns { ok, errors[] } and the parsed spec when valid.",
  {
    yaml: z.string().min(1),
  },
  async ({ yaml }) => {
    let spec;
    try {
      spec = parseYaml(yaml);
    } catch (err) {
      return {
        isError: true,
        content: [{ type: "text", text: `YAML parse error: ${err.message ?? err}` }],
      };
    }
    if (!spec || typeof spec !== "object") {
      return { isError: true, content: [{ type: "text", text: "Top-level YAML must be a mapping." }] };
    }
    const result = await validateSpec(spec);
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify({ ok: result.ok, errors: result.errors, spec: result.ok ? spec : undefined }, null, 2),
        },
      ],
    };
  }
);

// Tool: preview_capability_plan ----------------------------------------------
server.tool(
  "preview_capability_plan",
  "Static dry-run: predict the files / registry keys / processes a draft capability will touch. Does NOT execute anything. The user reviews this output before save_capability_yaml is called.",
  {
    yaml: z.string().min(1),
  },
  async ({ yaml }) => {
    let spec;
    try {
      spec = parseYaml(yaml);
    } catch (err) {
      return { isError: true, content: [{ type: "text", text: `YAML parse error: ${err.message ?? err}` }] };
    }
    const result = await validateSpec(spec);
    if (!result.ok) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: `Spec is not schema-valid; fix these errors first:\n${result.errors.map((e) => `  - ${e}`).join("\n")}`,
          },
        ],
      };
    }
    return { content: [{ type: "text", text: previewPlan(spec) }] };
  }
);

// Tool: save_capability_yaml --------------------------------------------------
server.tool(
  "save_capability_yaml",
  "Persist a schema-valid capability YAML to the user's local Moddin capabilities folder. The user then opens Moddin Desktop, clicks Rescan, and the new capability shows up as an installable option. Moddin's own preview/transaction layer takes over from there — this tool never touches the game directory.",
  {
    yaml: z.string().min(1),
    overwrite: z.boolean().default(false),
  },
  async ({ yaml, overwrite }) => {
    if (!LOCAL_CAPABILITIES_DIR) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: "LOCALAPPDATA is not set on this platform. Set MODDIN_LOCAL_CAPABILITIES_DIR to an explicit directory before calling save_capability_yaml.",
          },
        ],
      };
    }
    let spec;
    try {
      spec = parseYaml(yaml);
    } catch (err) {
      return { isError: true, content: [{ type: "text", text: `YAML parse error: ${err.message ?? err}` }] };
    }
    const result = await validateSpec(spec);
    if (!result.ok) {
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: `Refusing to save: spec is not schema-valid:\n${result.errors.map((e) => `  - ${e}`).join("\n")}`,
          },
        ],
      };
    }

    const filename = `${spec.id}.yaml`;
    const target = join(LOCAL_CAPABILITIES_DIR, filename);
    if (existsSync(target) && !overwrite) {
      return {
        isError: true,
        content: [
          { type: "text", text: `${target} already exists. Pass overwrite=true to replace (only do this after explicit user confirmation).` },
        ],
      };
    }
    await mkdir(LOCAL_CAPABILITIES_DIR, { recursive: true });
    // Re-emit with a stable key order; YAML serialization preserves comments from the input.
    const finalText = stringifyYaml(spec, { lineWidth: 0 });
    await writeFile(target, finalText, "utf8");
    return {
      content: [
        {
          type: "text",
          text: `Saved ${target}\nNext step: open Moddin Desktop → Rescan stores → the new capability "${spec.displayName}" appears under the matching game. Click Apply to preview & install.`,
        },
      ],
    };
  }
);

// Tool: moddin_status ---------------------------------------------------------
server.tool(
  "moddin_status",
  "Return diagnostic info: where capabilities will be saved, which built-in/local dirs were found, what version of the schema is loaded.",
  {},
  async () => {
    const builtIn = existsSync(BUILT_IN_CAPABILITIES_DIR);
    const games = existsSync(GAMES_CATALOG_DIR);
    const local = LOCAL_CAPABILITIES_DIR ? existsSync(LOCAL_CAPABILITIES_DIR) : false;
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            {
              projectRoot: PROJECT_ROOT,
              builtInCapabilitiesDir: BUILT_IN_CAPABILITIES_DIR,
              builtInFound: builtIn,
              gamesCatalogDir: GAMES_CATALOG_DIR,
              gamesFound: games,
              localCapabilitiesDir: LOCAL_CAPABILITIES_DIR,
              localDirExists: local,
              templatesDir: TEMPLATES_DIR,
              schemaPath: SCHEMA_PATH,
              supportedStepKinds: Object.keys(STEP_KINDS),
              supportedCheckKinds: Object.keys(CHECK_KINDS),
            },
            null,
            2
          ),
        },
      ],
    };
  }
);

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

const transport = new StdioServerTransport();
await server.connect(transport);

// Heartbeat so the parent process can see we are alive even before the first
// tool call (helps when the user starts the agent in a fresh terminal).
process.stderr.write(`moddin-mcp ready (project root: ${PROJECT_ROOT})\n`);