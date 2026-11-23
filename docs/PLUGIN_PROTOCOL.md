# UMMP Plugin Protocol — API 1

Plugins are untrusted, out-of-process executables. Install only plugins you trust. The protocol has no file-write operation; UMMP validates and applies proposed edits through MigrationEngine.

## Discovery and manifest

Place each plugin in its own immediate child directory of `<data_dir>/plugins/`. Each directory contains `ummp-plugin.toml`:

```toml
name = "demo-renamer"
version = "1.0.0"
kind = "rules" # "adapter" or "rules"
command = ["bin/demo-renamer"]
ummp_api = 1
```

The command is an argument array, not a shell string. Its executable must resolve to a regular file inside the plugin directory. Do not use `.jar` or `.class` executables.

Commands:

- `ummp plugins list`: list manifests in the default plugin directory.
- `ummp plugins check DIR`: validate the manifest, executable, API version and hello handshake.

## Framing

Each call starts a fresh child process. UMMP writes exactly one JSON request followed by LF to stdin, closes stdin, and expects exactly one non-empty JSON response line on stdout. Stderr is drained separately and is not protocol data. The process must exit with status 0.

### hello

Request: `{"method":"hello"}`

Response: `{"id":"demo-renamer","ummp_api":1}`

The response id must equal the manifest name and the API must equal 1.

### findings

Request:

```json
{"method":"findings","files":[{"path":"src/main/java/Demo.java","content":"class Demo {}"}]}
```

Response is an array of `TechFinding` values:

```json
[{"tech":"demo-renamer","handling":"assisted","file":"src/main/java/Demo.java","line":1,"message":"Demo type found"}]
```

### edits

Request:

```json
{"method":"edits","files":[{"path":"src/main/java/Demo.java","content":"class DemoWidget {}"}],"target":{"minecraft":"1.21.1","loader":"fabric"}}
```

Response:

```json
[{"path":"src/main/java/Demo.java","original":"class DemoWidget {}","updated":"class DemoGadget {}"}]
```

Every returned path must identify an existing file included in the request, remain inside the project root, and not be a `.jar` or `.class` file. The original string must exactly match the current file. UMMP converts these values to an `EditSet`; MigrationEngine takes a checkpoint and applies the edit set. A stale or unsafe edit is rejected.

## Limits and security

- 30 seconds per plugin call; the process tree is terminated on timeout.
- Maximum stdout response: 8 MiB. Stderr is drained but not parsed as protocol.
- The environment is cleared and rebuilt from the build runner's allow-list. API keys and tokens are not passed.
- Environment files, credential/secret/token configuration, package-manager auth files, SSH/cloud/container/Kubernetes credential directories, and private-key files (`.pem`, `.key`, `.p12`, `.pfx`, `.jks`, `.ppk`, `.keystore`) are excluded from request file contents. Source files such as `TokenManager.java` and `SecretManager.java` remain eligible.
- The protocol does not expose a file-write API. However, process execution is not an OS sandbox: a malicious plugin can still access files and processes permitted to the current user. Review the executable before installation.
