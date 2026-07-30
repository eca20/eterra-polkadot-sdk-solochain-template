import type {
  CompileFailure,
  CompileReport,
  CompileSuccess,
  FlowManifest,
} from "./types.js";

export interface WasmCompiler {
  compileManifest(manifestJson: string): string;
  compilerVersion?(): string;
}

export class FlowCompilationError extends Error {
  readonly report: CompileFailure;

  constructor(report: CompileFailure) {
    super(
      report.diagnostics
        .map((diagnostic) => `${diagnostic.path}: ${diagnostic.message}`)
        .join("\n"),
    );
    this.name = "FlowCompilationError";
    this.report = report;
  }
}

export function compileManifest(
  compiler: WasmCompiler,
  manifest: FlowManifest,
): CompileSuccess {
  const report = parseCompileReport(
    compiler.compileManifest(JSON.stringify(manifest)),
  );
  if (!report.ok) {
    throw new FlowCompilationError(report);
  }
  return report;
}

export function parseCompileReport(value: string): CompileReport {
  const parsed: unknown = JSON.parse(value);
  if (!isRecord(parsed) || typeof parsed.ok !== "boolean") {
    throw new TypeError("Flow compiler returned an invalid report");
  }
  if (!Array.isArray(parsed.diagnostics)) {
    throw new TypeError("Flow compiler report is missing diagnostics");
  }
  if (parsed.ok) {
    if (
      typeof parsed.scaleHex !== "string" ||
      !parsed.scaleHex.startsWith("0x") ||
      typeof parsed.manifestHashHex !== "string" ||
      !/^0x[0-9a-f]{64}$/.test(parsed.manifestHashHex)
    ) {
      throw new TypeError("Flow compiler success report is malformed");
    }
  }
  return parsed as unknown as CompileReport;
}

export function assertDeterministicCompilation(
  compiler: WasmCompiler,
  manifest: FlowManifest,
): CompileSuccess {
  const first = compileManifest(compiler, manifest);
  const second = compileManifest(compiler, manifest);
  if (
    first.scaleHex !== second.scaleHex ||
    first.manifestHashHex !== second.manifestHashHex ||
    first.canonicalAuthoringJson !== second.canonicalAuthoringJson
  ) {
    throw new Error("Flow compiler produced non-deterministic output");
  }
  return first;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
