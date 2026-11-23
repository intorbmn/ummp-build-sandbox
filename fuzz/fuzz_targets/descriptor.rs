#![no_main]

use libfuzzer_sys::fuzz_target;
use ummp_core::descriptor::{migrate_descriptor, DescriptorKind, DescriptorTarget};

fuzz_target!(|data: &[u8]| {
    if data.len() > 256 * 1024 || data.is_empty() {
        return;
    }
    let (kind, loader) = match data[0] {
        0 => (Some(DescriptorKind::Fabric), "fabric"),
        1 => (Some(DescriptorKind::Quilt), "quilt"),
        2 => (Some(DescriptorKind::ModsToml), "forge"),
        3 => (Some(DescriptorKind::NeoForgeModsToml), "neoforge"),
        // Spigot/Paper plugin.yml is YAML, not a UMMP migration descriptor yet.
        4 => {
            let _ = serde_yaml::from_slice::<serde_yaml::Value>(&data[1..]);
            return;
        }
        _ => (None, ""),
    };
    if let Some(kind) = kind {
        if let Ok(text) = std::str::from_utf8(&data[1..]) {
            let target = DescriptorTarget {
                minecraft: "1.20.1".into(),
                loader: loader.into(),
                loader_version: Some("0.15.0".into()),
            };
            let _ = migrate_descriptor(kind, text, &target);
        }
    }
});
