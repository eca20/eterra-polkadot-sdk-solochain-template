//! Browser-safe wrapper around the deterministic Rust manifest compiler.

use blockchainia_flow_manifest::{
    compile_manifest_json, CompilerDiagnostic, CompilerLimits, CostEstimate, GraphSummary,
    ManifestMetrics, AUTHORING_LABEL, ETERRA_AUTHORING_ALIAS,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SuccessReport {
    ok: bool,
    authoring_label: &'static str,
    permanent_alias: &'static str,
    canonical_authoring_json: String,
    scale_hex: String,
    manifest_hash_hex: String,
    metrics: ManifestMetrics,
    diagnostics: Vec<CompilerDiagnostic>,
    graph: GraphSummary,
    cost_estimates: Vec<CostEstimate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorReport {
    ok: bool,
    diagnostics: Vec<CompilerDiagnostic>,
}

/// Compile authoring JSON to the exact Manifest v0 SCALE payload.
///
/// The returned string is JSON so callers do not depend on JS object identity
/// or implicit wasm-bindgen serialization behavior.
#[wasm_bindgen(js_name = compileManifest)]
pub fn compile_manifest(manifest_json: &str) -> Result<String, JsValue> {
    compile_report(manifest_json).map_err(|message| JsValue::from_str(&message))
}

fn compile_report(manifest_json: &str) -> Result<String, String> {
    let report = match compile_manifest_json(manifest_json.as_bytes(), CompilerLimits::production())
    {
        Ok(compiled) => serde_json::to_string(&SuccessReport {
            ok: true,
            authoring_label: AUTHORING_LABEL,
            permanent_alias: ETERRA_AUTHORING_ALIAS,
            canonical_authoring_json: serde_json::to_string_pretty(&compiled.canonical_authoring)
                .map_err(|error| error.to_string())?,
            scale_hex: compiled.scale_hex(),
            manifest_hash_hex: compiled.manifest_hash_hex(),
            metrics: compiled.metrics,
            diagnostics: compiled.diagnostics,
            graph: compiled.graph,
            cost_estimates: compiled.cost_estimates,
        }),
        Err(diagnostics) => serde_json::to_string(&ErrorReport {
            ok: false,
            diagnostics,
        }),
    };
    report.map_err(|error| error.to_string())
}

#[wasm_bindgen(js_name = compilerVersion)]
pub fn compiler_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_wrapper_reports_locked_fixture() {
        let report = compile_report(include_str!("../../../examples/zelda-door.flow.json"))
            .expect("wrapper serializes");
        let report: serde_json::Value = serde_json::from_str(&report).expect("report parses");
        assert_eq!(report["ok"], true);
        assert_eq!(
            report["manifestHashHex"],
            "0x032251c5252f0d13230bd4a236cefcc6db32076502230fd03f70169cd402c433"
        );
    }
}
