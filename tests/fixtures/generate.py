#!/usr/bin/env python3
"""Generate fixture mod JARs for the analyzer test corpus (task 4.7).

Each fixture is a real zip archive containing the metadata file(s) the
analyzer inspects. Run: python3 tests/fixtures/generate.py
"""
import json
import os
import zipfile

OUT = os.path.join(os.path.dirname(__file__))


def write_jar(name, entries):
    path = os.path.join(OUT, name)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for arcname, content in entries.items():
            z.writestr(arcname, content)
    print(f"wrote {name}")


# --- Fabric fixtures ---
write_jar("fabric-a.jar", {
    "fabric.mod.json": json.dumps({
        "id": "fabrica",
        "version": "1.2.0",
        "name": "Fabrica",
        "depends": {"minecraft": "1.20.1", "fabricloader": ">=0.14.0", "fabric-api": ">=0.80.0"},
    }),
    "META-INF/MANIFEST.MF": "Implementation-Title: Fabrica\nImplementation-Version: 1.2.0\n",
})

write_jar("fabric-b.jar", {
    "fabric.mod.json": json.dumps({
        "id": "fabricb",
        "version": "0.9.3",
        "name": "FabricB",
        "depends": {"minecraft": "1.21", "fabrica": ">=1.0.0"},
    }),
})

write_jar("fabric-c.jar", {
    "fabric.mod.json": json.dumps({
        "id": "fabricc",
        "version": "2.0.0",
        "name": "FabricC",
        "depends": {"minecraft": "1.20.1", "fabricb": ">=0.9.0", "fabrica": ">=1.0.0"},
    }),
})

# --- Forge / NeoForge fixtures ---
write_jar("forge-a.jar", {
    "META-INF/mods.toml": """
modId = "forgea"
version = "1.4.0"
displayName = "ForgeA"
[[dependencies]]
modId = "minecraft"
versionRange = "1.20.1"
mandatory = true
[[dependencies]]
modId = "fabricb"
versionRange = ">=0.9.0"
mandatory = false
""".strip(),
    "META-INF/MANIFEST.MF": "Implementation-Title: ForgeA\n",
})

write_jar("neoforge-a.jar", {
    "META-INF/mods.toml": """
modId = "neoforgea"
version = "21.0.50"
displayName = "NeoForgeA"
loader = "NEOFORGE"
minecraft = "1.20.1"
""".strip(),
})

write_jar("forge-b.jar", {
    "META-INF/mods.toml": """
modId = "forgeb"
version = "0.7.1"
displayName = "ForgeB"
minecraft = "1.21"
""".strip(),
})

# --- Legacy mcmod.info fixture ---
write_jar("legacy-a.jar", {
    "mcmod.info": json.dumps([{
        "modid": "legacya",
        "version": "0.5.0",
        "name": "LegacyA",
        "mcversion": "1.12.2",
    }]),
})

# --- Manifest-only fallback fixture ---
write_jar("manifest-a.jar", {
    "META-INF/MANIFEST.MF": (
        "Implementation-Title: ManifestMod\n"
        "Implementation-Version: 3.1.4\n"
        "Mod-ID: manifestmod\n"
        "Build-Jdk: 17.0.12\n"
    ),
})

# --- Kotlin source fixture (no JAR metadata, only source scan) ---
os.makedirs(os.path.join(OUT, "kotlin-src"), exist_ok=True)
with open(os.path.join(OUT, "kotlin-src", "Example.kt"), "w") as f:
    f.write("fun main() { println(\"kotlin\") }\n")

# --- Empty JAR (no metadata) ---
write_jar("empty.jar", {})

# --- Real-world shaped fixtures (Phase 4 gap closure) ---
# A real mod JAR contains compiled classes and binary assets; the analyzer
# must only read metadata entries and never fail on binary content.
write_jar("fabric-classes.jar", {
    "fabric.mod.json": json.dumps({
        "schemaVersion": 1,
        "id": "fabricclasses",
        "version": "1.0.0",
        "name": "FabricClasses",
        "depends": {"minecraft": "1.20.1"},
    }),
    "com/example/FabricClasses.class": bytes([0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x41, 0xFF, 0xFE, 0x80]),
    "assets/fabricclasses/icon.png": bytes([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF]),
})

# NeoForge 1.20.5+ renamed the descriptor to META-INF/neoforge.mods.toml and,
# like real Forge mods.toml files, puts the Minecraft range in dependencies.
write_jar("neoforge-b.jar", {
    "META-INF/neoforge.mods.toml": """
modLoader = "javafml"
loaderVersion = "[4,)"
license = "MIT"
[[mods]]
modId = "neoforgeb"
version = "1.0.0"
displayName = "NeoForgeB"
[[dependencies.neoforgeb]]
modId = "neoforge"
type = "required"
versionRange = "[21.0,)"
[[dependencies.neoforgeb]]
modId = "minecraft"
type = "required"
versionRange = "[1.21,1.21.1)"
""".strip(),
    "com/example/NeoForgeB.class": bytes([0xCA, 0xFE, 0xBA, 0xBE, 0xFF]),
})

write_jar("quilt-a.jar", {
    "quilt.mod.json": json.dumps({
        "schema_version": 1,
        "quilt_loader": {
            "group": "com.example",
            "id": "quilta",
            "version": "2.0.0",
            "metadata": {"name": "QuiltA"},
            "depends": [
                {"id": "minecraft", "versions": ">=1.20.1"},
                {"id": "quilt_loader", "versions": ">=0.19.0"},
                "qsl",
            ],
        },
    }),
})

print("done")