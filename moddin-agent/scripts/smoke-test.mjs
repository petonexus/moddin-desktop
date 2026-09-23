#!/usr/bin/env node
// @ts-check
/**
 * Smoke test the moddin-mcp server over stdio.
 *
 *   1. Sends MCP `initialize`
 *   2. Sends `tools/list` and asserts every advertised tool is present
 *   3. Calls a few tools end-to-end (moddin_status, list_supported_games,
 *      list_capabilities, get_capability_template, validate_capability_yaml,
 *      preview_capability_plan)
 *
 * Exits non-zero on the first failure. Run with:
 *   node scripts/smoke-test.mjs
 */

import { spawn } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SERVER = resolve(__dirname, "..", "src", "mcp-server.mjs");

const PROJECT_ROOT =
  process.env.MODDIN_PROJECT_ROOT ?? resolve(__dirname, "..", "..");

const EXPECTED_TOOLS = [
  "list_supported_games",
  "get_game_info",
  "list_capabilities",
  "get_capability",
  "get_step_kinds",
  "get_check_kinds",
  "get_capability_template",
  "validate_capability_yaml",
  "preview_capability_plan",
  "save_capability_yaml",
  "moddin_status",
];

function spawnServer() {
  const child = spawn(process.execPath, [SERVER], {
    stdio: ["pipe", "pipe", "pipe"],
    env: {
      ...process.env,
      MODDIN_PROJECT_ROOT: PROJECT_ROOT,
    },
  });
  child.stderr.on("data", (chunk) => {
    process.stderr.write(`[server stderr] ${chunk}`);
  });
  return child;
}

class JsonRpc {
  constructor(child) {
    this.child = child;
    this.buffer = "";
    this.pending = new Map();
    this.nextId = 1;
    this.child.stdout.on("data", (chunk) => this._onData(chunk));
    this.child.on("exit", (code) => {
      const err = new Error(`server exited (code ${code})`);
      for (const { reject } of this.pending.values()) reject(err);
    });
  }

  _onData(chunk) {
    this.buffer += chunk.toString("utf8");
    let nl;
    while ((nl = this.buffer.indexOf("\n")) >= 0) {
      const line = this.buffer.slice(0, nl).trim();
      this.buffer = this.buffer.slice(nl + 1);
      if (!line) continue;
      let msg;
      try {
        msg = JSON.parse(line);
      } catch {
        continue;
      }
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(JSON.stringify(msg.error)));
        else resolve(msg.result);
      }
    }
  }

  request(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  }

  notify(method, params = {}) {
    this.child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
  }

  close() {
    this.child.kill();
  }
}

function assert(cond, msg) {
  if (!cond) {
    console.error(`✗ ${msg}`);
    process.exit(1);
  }
  console.log(`✓ ${msg}`);
}

const rpc = new JsonRpc(spawnServer());

try {
  // 1. initialize
  const init = await rpc.request("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "moddin-smoke-test", version: "0.0.1" },
  });
  assert(init.serverInfo?.name === "moddin", "initialize returned moddin serverInfo");
  rpc.notify("notifications/initialized");

  // 2. tools/list
  const tools = await rpc.request("tools/list", {});
  const advertised = new Set(tools.tools.map((t) => t.name));
  for (const t of EXPECTED_TOOLS) {
    assert(advertised.has(t), `tool advertised: ${t}`);
  }

  // 3a. moddin_status
  const status = await rpc.request("tools/call", {
    name: "moddin_status",
    arguments: {},
  });
  const statusPayload = JSON.parse(status.content[0].text);
  assert(statusPayload.supportedStepKinds.includes("extract-zip"), "status lists extract-zip step");
  assert(statusPayload.supportedCheckKinds.includes("archive-sha256"), "status lists archive-sha256 check");

  // 3b. list_supported_games
  const games = await rpc.request("tools/call", { name: "list_supported_games", arguments: {} });
  const gamesPayload = JSON.parse(games.content[0].text);
  assert(gamesPayload.count >= 1, `at least one supported game (got ${gamesPayload.count})`);
  const firstGame = gamesPayload.games[0];
  console.log(`    first game: ${firstGame.id} (${firstGame.name})`);

  // 3c. list_capabilities
  const caps = await rpc.request("tools/call", { name: "list_capabilities", arguments: { origin: "builtIn" } });
  const capsPayload = JSON.parse(caps.content[0].text);
  assert(capsPayload.count >= 3, `at least 3 built-in capabilities (got ${capsPayload.count})`);
  for (const c of capsPayload.capabilities) console.log(`    built-in: ${c.id}`);

  // 3d. get_capability_template + validate + preview (compose one end-to-end)
  const tplRes = await rpc.request("tools/call", {
    name: "get_capability_template",
    arguments: { pattern: "extract-zip" },
  });
  const tplYaml = tplRes.content[0].text;
  const tplSpec = parseYaml(tplYaml);
  // Fill in placeholders with valid values
  tplSpec.id = "smoke-test-mod";
  tplSpec.displayName = "Smoke Test Mod";
  tplSpec.status = "available";
  tplSpec.supportedEngines = ["unreal5"];
  // Fill the configSchema placeholders
  for (const f of tplSpec.configSchema) {
    if (f.name === "version") f.required = true;
    if (f.name === "expectedFile") {
      f.default = "Game/smoke-test.dll";
    }
  }
  const composed = stringifyYaml(tplSpec);
  const validation = await rpc.request("tools/call", {
    name: "validate_capability_yaml",
    arguments: { yaml: composed },
  });
  const validationPayload = JSON.parse(validation.content[0].text);
  assert(validationPayload.ok, "composed YAML passes validate_capability_yaml");
  assert(Array.isArray(validationPayload.errors) && validationPayload.errors.length === 0, "no validation errors");

  const preview = await rpc.request("tools/call", {
    name: "preview_capability_plan",
    arguments: { yaml: composed },
  });
  assert(preview.content[0].text.includes("Install plan"), "preview_capability_plan returns install plan");
  console.log("    preview first 6 lines:");
  for (const line of preview.content[0].text.split("\n").slice(0, 6)) {
    console.log(`      | ${line}`);
  }

  // 3e. Negative: invalid YAML should be rejected
  const badSpec = { id: "Bad ID with Spaces", displayName: "x", category: "wat", status: "available" };
  const badYaml = stringifyYaml(badSpec);
  const bad = await rpc.request("tools/call", {
    name: "validate_capability_yaml",
    arguments: { yaml: badYaml },
  });
  const badPayload = JSON.parse(bad.content[0].text);
  assert(badPayload.ok === false, "validate_capability_yaml rejects malformed spec");
  assert(badPayload.errors.length > 0, "rejected spec carries errors");

  console.log("\nAll smoke tests passed.");
} catch (err) {
  console.error("\n✗ smoke test failed:", err.message);
  process.exitCode = 1;
} finally {
  rpc.close();
}