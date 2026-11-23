#!/usr/bin/env python3
"""Build bounded seed corpora from checked-in fixtures for scheduled fuzz runs."""
import json
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests" / "fixtures"
FUZZ = ROOT / "fuzz"
TARGETS = (
    "class_file",
    "descriptor",
    "mrpack_index",
    "zip_import",
    "gradle_log",
    "crash_report",
    "plugin_protocol",
    "rest_request",
    "mcp_request",
)


def corpus(name: str) -> Path:
    path = FUZZ / "corpus" / name
    path.mkdir(parents=True, exist_ok=True)
    return path


def seed_class_files() -> None:
    output = corpus("class_file")
    with zipfile.ZipFile(FIXTURES / "fabric-classes.jar") as archive:
        classes = [name for name in archive.namelist() if name.endswith(".class")]
        for index, name in enumerate(classes[:16]):
            data = archive.read(name)
            if len(data) <= 1024 * 1024:
                (output / f"class-{index:02}.class").write_bytes(data)


def seed_descriptors() -> None:
    output = corpus("descriptor")
    (output / "fabric.json").write_bytes(
        b"\x00" + (FIXTURES / "loader/fabric-src/src/main/resources/fabric.mod.json").read_bytes()
    )
    (output / "mods.toml").write_bytes(
        b"\x02" + (FIXTURES / "forge-src-1.16/src/main/resources/META-INF/mods.toml").read_bytes()
    )
    (output / "plugin.yml").write_bytes(
        b"\x04" + b"name: ExamplePlugin\nversion: 1.0.0\nmain: com.example.Plugin\n"
    )
    with zipfile.ZipFile(FIXTURES / "quilt-a.jar") as archive:
        for name in archive.namelist():
            if name.endswith("quilt.mod.json"):
                (output / "quilt.json").write_bytes(b"\x01" + archive.read(name))
                break


def seed_mrpack() -> None:
    output = corpus("mrpack_index")
    index = {
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "fuzz-seed",
        "name": "Fuzz seed",
        "files": [],
        "dependencies": {"minecraft": "1.20.1", "fabric-loader": "0.15.0"},
    }
    with zipfile.ZipFile(output / "valid-index.mrpack", "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("modrinth.index.json", json.dumps(index))


def seed_zip_import() -> None:
    output = corpus("zip_import")
    (output / "fabric-a.zip").write_bytes((FIXTURES / "fabric-a.jar").read_bytes())


def seed_logs() -> None:
    gradle = corpus("gradle_log")
    crash = corpus("crash_report")
    for path in (FIXTURES / "logs").glob("*.log"):
        (gradle / path.name).write_bytes(path.read_bytes())
    (crash / "crash-report.txt").write_bytes(
        (FIXTURES / "logs/crash-report.txt").read_bytes()
    )


def seed_api_requests() -> None:
    rest = corpus("rest_request")
    mcp = corpus("mcp_request")
    rest_samples = {
        "path.json": b'\x00{"path":"/tmp/project"}',
        "plan.json": b'\x01{"path":"/tmp/project","to":"1.21.1","loader":"fabric"}',
        "migrate.json": b'\x02{"path":"/tmp/project","to":"1.21.1","loader":"fabric","reviewed":false}',
        "port.json": b'\x03{"input":"/tmp/project","output":"/tmp/output","to":"1.21.1","loader":"fabric"}',
        "build.json": b'\x05{"path":"/tmp/project"}',
        "rollback.json": b'\x06{"checkpoint":null}',
    }
    mcp_samples = {
        "path.json": b'\x00{"path":"/tmp/project"}',
        "plan.json": b'\x01{"path":"/tmp/project","to":"1.21.1","loader":"fabric"}',
        "port.json": b'\x02{"path":"/tmp/project","to":"1.21.1","loader":"fabric","approve":false}',
        "build.json": b'\x03{"path":"/tmp/project","backend":"github"}',
        "job.json": b'\x04{"id":"00000000-0000-0000-0000-000000000000"}',
        "rollback.json": b'\x05{"project":"/tmp/project","confirm":false}',
    }
    for name, data in rest_samples.items():
        (rest / name).write_bytes(data)
    for name, data in mcp_samples.items():
        (mcp / name).write_bytes(data)


def seed_plugin_protocol() -> None:
    output = corpus("plugin_protocol")
    (output / "hello.jsonl").write_text(
        '{"ummp_api":1,"id":"fuzz-seed","result":{}}\n', encoding="utf-8"
    )
    (output / "empty-object.jsonl").write_text("{}\n", encoding="utf-8")


if __name__ == "__main__":
    for target in TARGETS:
        corpus(target)
    seed_class_files()
    seed_descriptors()
    seed_mrpack()
    seed_zip_import()
    seed_logs()
    seed_plugin_protocol()
    seed_api_requests()
