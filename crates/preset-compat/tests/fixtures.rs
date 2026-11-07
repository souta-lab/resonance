//! Fixture-driven compatibility tests.

use std::path::PathBuf;

use preset_compat::{AssetDirs, ExportOptions, SourceFormat};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/presets")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixtures().join(name)).expect("fixture exists")
}

#[test]
fn every_fixture_parses() {
    for name in [
        "bass-boost.json",
        "podcast-voice.json",
        "legacy-format.json",
        "unknown-keys.json",
        "missing-ir.json",
        "convolver.json",
    ] {
        let preset = preset_compat::parse(&read(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            !preset.plugins_order.is_empty(),
            "{name} should have at least one effect"
        );
    }
}

#[test]
fn roundtrip_preserves_instances_and_unknown_keys() {
    let text = read("unknown-keys.json");
    let preset = preset_compat::parse(&text).unwrap();
    let exported = preset_compat::to_json(&preset, ExportOptions::default());
    let again = preset_compat::parse(&exported).unwrap();
    assert_eq!(preset.plugins_order, again.plugins_order);
    let eq = again.instances.get("equalizer#0").unwrap();
    assert_eq!(
        eq.values.get("future-key").unwrap(),
        &serde_json::json!({"nested": [1, 2, 3], "flag": true})
    );
    assert!(again.warnings.iter().any(|w| w.code == "unknown_plugin"));
}

#[test]
fn legacy_fixture_normalizes_and_exports_legacy() {
    let preset = preset_compat::parse(&read("legacy-format.json")).unwrap();
    assert_eq!(preset.source_format, SourceFormat::Legacy);
    assert_eq!(preset.plugins_order, vec!["equalizer#0", "limiter#0"]);
    let text = preset_compat::to_json(
        &preset,
        ExportOptions {
            legacy: true,
            sort_keys: true,
        },
    );
    assert!(text.contains("\"equalizer\""));
    assert!(!text.contains("equalizer#0"));
}

#[test]
fn missing_ir_is_reported() {
    let preset = preset_compat::parse(&read("missing-ir.json")).unwrap();
    let missing = preset_compat::resolve_assets(&preset, &AssetDirs::default());
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].name, "studio-hall");
    assert_eq!(missing[0].instance, "convolver#0");
}

#[test]
fn convolver_fixture_resolves_its_ir() {
    let preset = preset_compat::parse(&read("convolver.json")).unwrap();
    let dirs = AssetDirs {
        irs: vec![fixtures().join("../irs")],
        models: Vec::new(),
    };
    let missing = preset_compat::resolve_assets(&preset, &dirs);
    assert!(missing.is_empty(), "{missing:?}");
}

#[test]
fn bass_boost_applies_to_a_store() {
    let preset = preset_compat::parse(&read("bass-boost.json")).unwrap();
    let instance = preset.instances.get("equalizer#0").unwrap();
    let desc = dsp_effects::catalog::descriptor(dsp_core::EffectKind::Equalizer);
    let store = dsp_core::param::ParamStore::new(desc.params, desc.bands);
    let warnings = preset_compat::apply_instance(instance, &store);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(store.band_value(0, 0, 1), 60.0);
    assert_eq!(store.band_value(0, 0, 2), 6.0);
    assert_eq!(store.band_value(1, 0, 2), 6.0);
}
