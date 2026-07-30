import {
  FLOW_AUTHORING_LABEL,
  assertDeterministicCompilation,
  preparePublishPlan,
  type CompileSuccess,
  type CompilerDiagnostic,
  type FlowManifest,
  type WasmCompiler,
} from "@blockchainia/flow-sdk";

import {
  cloneManifest,
  exportManifest,
  importManifest,
  starterManifest,
  validateDraft,
} from "./model.js";
import "./styles.css";

interface WasmModule {
  default(input?: unknown): Promise<unknown>;
  compileManifest(value: string): string;
  compilerVersion(): string;
}

let manifest = cloneManifest(starterManifest);
let compiled: CompileSuccess | undefined;
let compiler: WasmCompiler | undefined;

const root = document.querySelector<HTMLDivElement>("#app");
if (root === null) {
  throw new Error("Missing app root");
}

root.innerHTML = `
  <header class="topbar">
    <a class="brand" href="/" aria-label="Blockchainia Flow">
      <span class="brand-mark">B/</span>
      <span>FLOW</span>
    </a>
    <div class="release">
      <span class="pulse"></span>
      Private alpha · 0.1.0-alpha.1
    </div>
    <div class="custody">Key custody: external</div>
  </header>
  <main>
    <section class="hero">
      <div>
        <p class="eyebrow">Bounded orchestration / runtime authoritative</p>
        <h1>Design the path.<br /><em>Keep the chain in charge.</em></h1>
      </div>
      <p class="hero-copy">
        Compose states, conditions, economy gates, and effects. The builder
        validates and prepares unsigned calls; it never stores a key or submits
        a transaction.
      </p>
    </section>
    <section class="workspace">
      <aside class="rail panel">
        <div class="panel-title"><span>01</span> Manifest</div>
        <label>Template
          <select id="template">
            <option value="door">Door unlock</option>
            <option value="blank">Blank machine</option>
          </select>
        </label>
        <div class="two-up">
          <label>Game ID<input id="game-id" type="number" min="0" /></label>
          <label>Version<input id="version-id" type="number" min="0" /></label>
        </div>
        <label>Authoring contract
          <select id="label">
            <option value="blockchainia.flow.v0">blockchainia.flow.v0</option>
            <option value="eterra.flow.v0">eterra.flow.v0 (permanent alias)</option>
          </select>
        </label>
        <div class="panel-title spaced"><span>02</span> Transition</div>
        <div class="two-up">
          <label>From<input id="from-state" type="number" /></label>
          <label>To<input id="to-state" type="number" /></label>
        </div>
        <label>Priority<input id="priority" type="number" min="0" /></label>
        <div class="button-grid">
          <button id="add-condition" class="quiet">+ condition</button>
          <button id="add-effect" class="quiet">+ effect</button>
        </div>
        <p id="edit-summary" class="micro"></p>
      </aside>
      <section class="canvas panel">
        <div class="panel-head">
          <div class="panel-title"><span>03</span> State map</div>
          <span id="compiler-status" class="status">Loading compiler…</span>
        </div>
        <div id="state-map" class="state-map"></div>
        <div class="flow-sequence">
          <div><b>Action</b><small>client intent</small></div>
          <i>→</i>
          <div><b>Validate</b><small>runtime rules</small></div>
          <i>→</i>
          <div><b>Effect</b><small>provider calls</small></div>
        </div>
      </section>
      <aside class="inspector panel">
        <div class="panel-title"><span>04</span> Validate</div>
        <div id="diagnostics" class="diagnostics"></div>
        <button id="compile" class="primary">Compile deterministic SCALE</button>
        <div id="artifact" class="artifact"></div>
        <button id="prepare" class="secondary" disabled>Prepare unsigned calls</button>
      </aside>
    </section>
    <section class="json-panel panel">
      <div class="panel-head">
        <div class="panel-title"><span>05</span> Import / export</div>
        <div class="button-grid">
          <button id="import" class="quiet">Import JSON</button>
          <button id="export" class="quiet">Export JSON</button>
        </div>
      </div>
      <textarea id="json-editor" spellcheck="false" aria-label="Flow manifest JSON"></textarea>
    </section>
    <section id="publish-plan" class="publish-plan panel hidden"></section>
  </main>
  <footer>
    <span>BLOCKCHAINIA FLOW</span>
    <span>Preview ≠ runtime acceptance</span>
    <span>No wallet · no signer · no RPC submit</span>
  </footer>
`;

const editor = getElement<HTMLTextAreaElement>("json-editor");
const prepareButton = getElement<HTMLButtonElement>("prepare");

function render(): void {
  getElement<HTMLInputElement>("game-id").value = String(manifest.game_id);
  getElement<HTMLInputElement>("version-id").value = String(
    manifest.version_id,
  );
  getElement<HTMLSelectElement>("label").value = manifest.manifest_version;
  const transition = manifest.transitions[0];
  getElement<HTMLInputElement>("from-state").value = String(
    transition?.from_state ?? "",
  );
  getElement<HTMLInputElement>("to-state").value = String(
    transition?.to_state ?? "",
  );
  getElement<HTMLInputElement>("priority").value = String(
    transition?.priority ?? 0,
  );
  editor.value = exportManifest(manifest);
  renderMap();
  renderDiagnostics(validateDraft(manifest));
  getElement("edit-summary").textContent = transition
    ? `${transition.conditions.length} condition(s) · ${transition.effects.length} effect(s)`
    : "Add a transition in JSON to enable transition controls.";
}

function renderMap(): void {
  const machine = manifest.machines[0];
  const transition = manifest.transitions[0];
  const stateMap = getElement("state-map");
  if (machine === undefined) {
    stateMap.innerHTML = `<div class="empty">Add a machine to begin.</div>`;
    return;
  }
  stateMap.innerHTML = machine.states
    .map((state) => {
      const role =
        state === machine.initial_state
          ? "initial"
          : state === transition?.to_state
            ? "target"
            : "";
      return `
        <div class="state-node ${role}">
          <span>STATE</span>
          <strong>${state}</strong>
          <small>${role || "reachable"}</small>
        </div>
        ${
          state !== machine.states.at(-1)
            ? `<div class="edge"><span>action ${transition?.action_id ?? "—"}</span>→</div>`
            : ""
        }
      `;
    })
    .join("");
}

function renderDiagnostics(diagnostics: CompilerDiagnostic[]): void {
  const target = getElement("diagnostics");
  if (diagnostics.length === 0) {
    target.innerHTML = `<div class="diagnostic ok">Draft checks clear</div>`;
    return;
  }
  target.innerHTML = diagnostics
    .map(
      (diagnostic) => `
        <div class="diagnostic ${diagnostic.severity}">
          <b>${escapeHtml(diagnostic.code)}</b>
          <span>${escapeHtml(diagnostic.message)}</span>
          <code>${escapeHtml(diagnostic.path)}</code>
        </div>
      `,
    )
    .join("");
}

function syncSimpleFields(): void {
  manifest.game_id = numericValue("game-id");
  manifest.version_id = numericValue("version-id");
  manifest.manifest_version = getElement<HTMLSelectElement>("label")
    .value as FlowManifest["manifest_version"];
  const transition = manifest.transitions[0];
  if (transition !== undefined) {
    transition.from_state = nullableNumericValue("from-state");
    transition.to_state = nullableNumericValue("to-state");
    transition.priority = numericValue("priority");
  }
  compiled = undefined;
  prepareButton.disabled = true;
  render();
}

for (const id of [
  "game-id",
  "version-id",
  "label",
  "from-state",
  "to-state",
  "priority",
]) {
  getElement(id).addEventListener("change", syncSimpleFields);
}

getElement("add-condition").addEventListener("click", () => {
  manifest.transitions[0]?.conditions.push({
    atom: {
      machine_state_equals: {
        scope: { instance: true },
        machine_id: manifest.machines[0]?.machine_id ?? 0,
        state_id: manifest.machines[0]?.initial_state ?? 0,
      },
    },
  });
  render();
});

getElement("add-effect").addEventListener("click", () => {
  manifest.transitions[0]?.effects.push({
    set_machine_state: {
      scope: { instance: true },
      machine_id: manifest.machines[0]?.machine_id ?? 0,
      state_id: manifest.transitions[0]?.to_state ?? 0,
    },
  });
  render();
});

getElement("template").addEventListener("change", (event) => {
  const value = (event.currentTarget as HTMLSelectElement).value;
  manifest =
    value === "blank"
      ? {
          ...cloneManifest(starterManifest),
          variables: [],
          transitions: [],
        }
      : cloneManifest(starterManifest);
  compiled = undefined;
  render();
});

getElement("import").addEventListener("click", () => {
  try {
    manifest = importManifest(editor.value);
    compiled = undefined;
    render();
  } catch (error) {
    renderDiagnostics([
      {
        severity: "error",
        code: "invalid_import",
        path: "manifest",
        message: error instanceof Error ? error.message : String(error),
      },
    ]);
  }
});

getElement("export").addEventListener("click", () => {
  const blob = new Blob([exportManifest(manifest)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `flow-${manifest.game_id}-${manifest.version_id}.json`;
  link.click();
  URL.revokeObjectURL(url);
});

getElement("compile").addEventListener("click", () => {
  if (compiler === undefined) {
    renderDiagnostics([
      {
        severity: "error",
        code: "compiler_unavailable",
        path: "compiler",
        message:
          "Build the Rust WASM package with scripts/build-wasm.sh, then reload.",
      },
    ]);
    return;
  }
  try {
    manifest = importManifest(editor.value);
    compiled = assertDeterministicCompilation(compiler, manifest);
    renderDiagnostics(compiled.diagnostics);
    getElement("artifact").innerHTML = `
      <dl>
        <dt>Bytes</dt><dd>${compiled.metrics.scaleBytes}</dd>
        <dt>Hash</dt><dd><code>${compiled.manifestHashHex.slice(0, 18)}…</code></dd>
        <dt>Label</dt><dd>${FLOW_AUTHORING_LABEL}</dd>
      </dl>
    `;
    prepareButton.disabled = false;
    renderMap();
  } catch (error) {
    renderDiagnostics(
      "report" in (error as object)
        ? (error as { report: { diagnostics: CompilerDiagnostic[] } }).report
            .diagnostics
        : [
            {
              severity: "error",
              code: "compile_failed",
              path: "manifest",
              message: error instanceof Error ? error.message : String(error),
            },
          ],
    );
  }
});

prepareButton.addEventListener("click", () => {
  if (compiled === undefined) return;
  const plan = preparePublishPlan(compiled, {
    gameId: manifest.game_id,
    versionId: manifest.version_id,
  });
  const target = getElement("publish-plan");
  target.classList.remove("hidden");
  target.innerHTML = `
    <div class="panel-head">
      <div class="panel-title"><span>06</span> Unsigned publish plan</div>
      <b>${plan.calls.length} calls · external signer required</b>
    </div>
    <ol>${plan.calls
      .map(
        (call) =>
          `<li><code>29:${call.callIndex}</code><span>${call.call}</span></li>`,
      )
      .join("")}</ol>
  `;
  target.scrollIntoView({ behavior: "smooth" });
});

async function loadCompiler(): Promise<void> {
  try {
    const modulePath =
      "/manifest-wasm/blockchainia_flow_manifest_wasm.js";
    const wasm = (await import(/* @vite-ignore */ modulePath)) as WasmModule;
    await wasm.default();
    compiler = {
      compileManifest: (value) => wasm.compileManifest(value),
      compilerVersion: () => wasm.compilerVersion(),
    };
    getElement("compiler-status").textContent =
      `WASM ${compiler.compilerVersion?.() ?? "ready"}`;
    getElement("compiler-status").classList.add("ready");
  } catch {
    getElement("compiler-status").textContent = "WASM build required";
  }
}

function numericValue(id: string): number {
  return Number(getElement<HTMLInputElement>(id).value);
}

function nullableNumericValue(id: string): number | null {
  const value = getElement<HTMLInputElement>(id).value;
  return value === "" ? null : Number(value);
}

function getElement<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (element === null) throw new Error(`Missing #${id}`);
  return element as T;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#039;",
    };
    return entities[character] ?? character;
  });
}

render();
void loadCompiler();
