//! MCP stdio adapter. All domain operations are delegated to AppServices.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult, ContentBlock, ListResourceTemplatesResult, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, ResourceContents, ResourceTemplate,
        ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData, RoleServer, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::{
    error::Error,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};
use ummp_core::app::{
    views::{JobView, RiskLevel},
    AppError, AppServices, BuildJobRequest, PlanRequest, PortRequest,
};
use uuid::Uuid;

/// Runs the MCP server over stdin/stdout. Stdout is reserved for protocol frames.
pub fn serve(
    services: Arc<AppServices>,
    allowed_roots: Vec<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let server = McpServer::new(services, allowed_roots)
            .serve(stdio())
            .await?;
        server.waiting().await?;
        Ok::<(), Box<dyn Error>>(())
    })?;
    Ok(())
}

#[derive(Clone)]
struct McpServer {
    services: Arc<AppServices>,
    roots: Vec<PathBuf>,
}

impl McpServer {
    fn new(services: Arc<AppServices>, roots: Vec<PathBuf>) -> Self {
        Self {
            services,
            roots: super::canonical_roots(roots),
        }
    }

    fn path(&self, path: &Path) -> Result<PathBuf, AppError> {
        super::resolve_allowed_path(path, &self.roots).map_err(AppError::Refused)
    }

    const fn plan_request(path: PathBuf, to: String, loader: String) -> PlanRequest {
        PlanRequest {
            input: path,
            from: None,
            to,
            loader: Some(loader),
            min_confidence: 0.8,
            source_loader: None,
            kb: None,
            mappings: None,
            snapshots: None,
        }
    }
}

fn result<T: Serialize>(value: Result<T, AppError>) -> Result<CallToolResult, ErrorData> {
    match value {
        Ok(value) => {
            let value = serde_json::to_value(value)
                .map_err(|_| ErrorData::internal_error("could not serialize tool result", None))?;
            Ok(CallToolResult::structured(value))
        }
        Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(
            error.to_string(),
        )])),
    }
}

fn report_markdown(job: &JobView) -> String {
    let mut report = format!(
        "# UMMP job report\n\n- Job: {}\n- State: {:?}\n- Step: {}\n",
        job.id, job.state, job.step
    );
    if let Some(progress) = &job.progress {
        let _ = writeln!(report, "- Progress: {}/{}", progress.done, progress.total);
    }
    if let Some(error) = &job.error {
        let _ = writeln!(report, "\n## Error\n\n{error}");
    }
    if let Some(value) = &job.result {
        report.push_str("\n## Result\n\n```json\n");
        report.push_str(&serde_json::to_string_pretty(value).unwrap_or_default());
        report.push_str("\n```\n");
    }
    if !job.log.is_empty() {
        report.push_str("\n## Log\n");
        for line in &job.log {
            let _ = writeln!(report, "- {line}");
        }
    }
    report
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PathArgs {
    /// Absolute or relative path inside configured allowed_roots.
    path: PathBuf,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PlanArgs {
    /// Source project path inside configured allowed_roots.
    path: PathBuf,
    /// Target Minecraft version, for example 1.21.1.
    to: String,
    /// Target loader: fabric, forge, neoforge or quilt.
    loader: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PortArgs {
    /// Source project path inside configured allowed_roots.
    path: PathBuf,
    to: String,
    loader: String,
    /// Optional output directory, also restricted to allowed_roots.
    output: Option<PathBuf>,
    /// Create a ZIP of the ported output.
    #[serde(default)]
    zip: bool,
    /// Must be true to start writing. False returns a read-only plan.
    #[serde(default)]
    approve: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct BuildArgs {
    /// Source project path inside configured allowed_roots.
    path: PathBuf,
    /// "github" (default) or "local". Local builds are allowed only for Low risk.
    #[serde(default = "default_backend")]
    backend: String,
}

fn default_backend() -> String {
    "github".into()
}

#[derive(Debug, Deserialize, JsonSchema)]
struct JobArgs {
    /// UUID returned by a start_port or build operation.
    id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RollbackArgs {
    /// Project path inside configured allowed_roots.
    project: PathBuf,
    /// Optional checkpoint UUID; omitted means the latest checkpoint.
    checkpoint: Option<String>,
    /// Must be true to restore files.
    #[serde(default)]
    confirm: bool,
}

#[tool_router]
impl McpServer {
    #[tool(
        description = "Analyze a Minecraft mod project or JAR. Read-only; path must be inside allowed_roots."
    )]
    fn ummp_analyze(
        &self,
        Parameters(PathArgs { path }): Parameters<PathArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.path(&path);
        result(path.and_then(|path| self.services.analyze(&path)))
    }

    #[tool(
        description = "Scan a mod project or JAR for malware-risk indicators. Read-only; path must be inside allowed_roots."
    )]
    fn ummp_scan(
        &self,
        Parameters(PathArgs { path }): Parameters<PathArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.path(&path);
        result(path.and_then(|path| self.services.scan(&path)))
    }

    #[tool(
        description = "Create a read-only migration plan. Returns a summary and at most 40 diff lines per file; does not write files."
    )]
    fn ummp_plan(
        &self,
        Parameters(PlanArgs { path, to, loader }): Parameters<PlanArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let request = self
            .path(&path)
            .map(|path| Self::plan_request(path, to, loader));
        let plan = request.and_then(|request| self.services.plan(&request));
        let value = plan.map(|plan| {
            serde_json::json!({
                "source_minecraft": plan.source_minecraft,
                "target_minecraft": plan.target_minecraft,
                "source_loader": plan.source_loader,
                "target_loader": plan.target_loader,
                "files_scanned": plan.files_scanned,
                "files": plan.files.iter().map(|file| serde_json::json!({
                    "path": file.path,
                    "kind": file.kind,
                    "replacements": file.replacements,
                    "needs_attention": file.needs_attention,
                    "diff_preview": file.diff.lines().take(40).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "moves": plan.moves,
                "deletes": plan.deletes,
                "warnings": plan.warnings,
                "skipped": plan.skipped,
            })
        });
        result(value)
    }

    #[tool(
        description = "Port a mod. With approve=false (default), returns a read-only plan. Set approve=true only after the human explicitly approves; then a job is started."
    )]
    fn ummp_port(
        &self,
        Parameters(PortArgs {
            path,
            to,
            loader,
            output,
            zip,
            approve,
        }): Parameters<PortArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let input = self.path(&path);
        let output = output.map(|path| self.path(&path)).transpose();
        if !approve {
            let plan = input
                .and_then(|input| output.map(|_| Self::plan_request(input, to, loader)))
                .and_then(|request| self.services.plan(&request));
            return result(plan.map(|plan| serde_json::json!({
                "approved": false,
                "writes_started": false,
                "plan": plan,
                "next_step": "Ask the human for approval, then call ummp_port again with approve=true."
            })));
        }
        let request = input.and_then(|input| {
            let output = output?;
            Ok(PortRequest {
                input,
                output,
                to,
                from: None,
                loader: Some(loader),
                zip,
                min_confidence: 0.8,
                kb: None,
                mappings: None,
            })
        });
        result(request.and_then(|request| {
            self.services
                .start_port(request)
                .map(|id| serde_json::json!({ "job": id, "approved": true }))
        }))
    }

    #[tool(
        description = "Build and test a mod project. GitHub Actions is the default backend. Local execution is refused unless AppServices rates the project Low risk."
    )]
    fn ummp_build(
        &self,
        Parameters(BuildArgs { path, backend }): Parameters<BuildArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = self.path(&path);
        let request = path.and_then(|path| {
            let local = match backend.as_str() {
                "github" => false,
                "local" => {
                    let risk = self.services.check_build_risk(&path, true, false, false)?;
                    if risk.is_some_and(|level| level != RiskLevel::Low) {
                        return Err(AppError::Refused(
                            "local builds require Low risk; use the GitHub backend".into(),
                        ));
                    }
                    true
                }
                _ => return Err(AppError::Input("backend must be github or local".into())),
            };
            Ok(BuildJobRequest {
                path,
                local,
                repo: None,
                stages: vec!["build".into(), "test".into(), "server".into()],
                timeout_secs: 1800,
                java: None,
                workdir: None,
                accept_high_risk: false,
                confirm_medium: false,
            })
        });
        result(request.and_then(|request| {
            self.services
                .start_build(request)
                .map(|id| serde_json::json!({ "job": id }))
        }))
    }

    #[tool(description = "Get the current state, progress, recent logs and result of a UMMP job.")]
    fn ummp_job(
        &self,
        Parameters(JobArgs { id }): Parameters<JobArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = Uuid::parse_str(&id)
            .map_err(|_| ErrorData::invalid_params("id must be a valid UUID", None))?;
        result(self.services.job(id))
    }

    #[tool(description = "Request cancellation of a running UMMP job.")]
    fn ummp_cancel(
        &self,
        Parameters(JobArgs { id }): Parameters<JobArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = Uuid::parse_str(&id)
            .map_err(|_| ErrorData::invalid_params("id must be a valid UUID", None))?;
        result(self.services.cancel(id).map(|()| {
            serde_json::json!({
                "job": id,
                "cancel_requested": true
            })
        }))
    }

    #[tool(
        description = "Restore a project checkpoint. Requires confirm=true after the human explicitly approves the rollback."
    )]
    fn ummp_rollback(
        &self,
        Parameters(RollbackArgs {
            project,
            checkpoint,
            confirm,
        }): Parameters<RollbackArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        if !confirm {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "Rollback refused: obtain explicit human approval and call again with confirm=true.",
            )]));
        }
        let project = self.path(&project);
        let checkpoint = checkpoint
            .map(|id| {
                Uuid::parse_str(&id)
                    .map_err(|_| ErrorData::invalid_params("checkpoint must be a valid UUID", None))
            })
            .transpose()?;
        result(project.and_then(|project| {
            self.services
                .rollback_with_snapshots(&project, checkpoint, None)
                .map(|id| serde_json::json!({ "restored": true, "checkpoint": id }))
        }))
    }
}

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(
            "UMMP local MCP server. Paths are restricted to allowed_roots. Porting requires approve=true and rollback requires confirm=true.",
        )
    }

    async fn list_resource_templates(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new("ummp://report/{job}", "job-report")
                .with_description("Markdown report for a UMMP job UUID.")
                .with_mime_type("text/markdown"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let id = request
            .uri
            .strip_prefix("ummp://report/")
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| ErrorData::invalid_params("expected ummp://report/{job-uuid}", None))?;
        let job = self
            .services
            .job(id)
            .map_err(|_| ErrorData::resource_not_found("UMMP job report not found", None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            report_markdown(&job),
            &request.uri,
        )])
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_rejects_paths_outside_canonical_allowed_roots() {
        let allowed = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let services = Arc::new(
            AppServices::open(ummp_core::app::Settings::new(data.path().to_path_buf())).unwrap(),
        );
        let server = McpServer::new(services, vec![allowed.path().to_path_buf()]);

        assert!(server.path(allowed.path()).is_ok());
        assert!(server.path(outside.path()).is_err());
    }

    #[test]
    fn port_without_approval_returns_plan_and_writes_nothing() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/demo-fabric-mod");
        let fixtures_root = fixture.parent().unwrap();
        let descriptor = fixture.join("src/main/resources/fabric.mod.json");
        let before = std::fs::read(&descriptor).unwrap();
        let output = fixtures_root.join("ported-preview");
        let data = tempfile::tempdir().unwrap();

        let services = Arc::new(
            AppServices::open(ummp_core::app::Settings::new(data.path().to_path_buf())).unwrap(),
        );
        let server = McpServer::new(services, vec![fixtures_root.to_path_buf()]);
        let response = server
            .ummp_port(Parameters(PortArgs {
                path: fixture,
                to: "1.21.1".into(),
                loader: "fabric".into(),
                output: Some(output.clone()),
                zip: false,
                approve: false,
            }))
            .unwrap();

        let value = serde_json::to_value(response).unwrap();
        assert_ne!(value["isError"], true);
        assert_eq!(std::fs::read(&descriptor).unwrap(), before);
        assert!(!output.exists());
    }

    #[test]
    fn port_preview_rejects_output_outside_canonical_allowed_roots() {
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/demo-fabric-mod");
        let outside = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let output = outside.path().join("ported");
        let services = Arc::new(
            AppServices::open(ummp_core::app::Settings::new(data.path().to_path_buf())).unwrap(),
        );
        let server = McpServer::new(services, vec![fixture.parent().unwrap().to_path_buf()]);
        let response = server
            .ummp_port(Parameters(PortArgs {
                path: fixture,
                to: "1.21.1".into(),
                loader: "fabric".into(),
                output: Some(output.clone()),
                zip: false,
                approve: false,
            }))
            .unwrap();

        let value = serde_json::to_value(response).unwrap();
        assert!(value["isError"].as_bool().unwrap());
        assert!(!output.exists());
    }

    /// The confirm gate itself must be what stops the rollback. The project has a real
    /// checkpoint inside the allowed root, so without the gate the restore would succeed.
    #[test]
    fn rollback_with_real_checkpoint_needs_confirm() {
        use ummp_core::app::snapshots::{default_snapshots, save_store};
        use ummp_core::checkpoint::CheckpointStore;

        let allowed = tempfile::tempdir().unwrap();
        let project = allowed.path().join("demo");
        std::fs::create_dir_all(&project).unwrap();
        let file = project.join("DemoWidget.java");
        std::fs::write(&file, "class DemoWidget {}").unwrap();

        let snaps = default_snapshots(&project);
        let mut store = CheckpointStore::new();
        store
            .snapshot(Uuid::new_v4(), "before-edit", &project, &snaps)
            .unwrap();
        save_store(&snaps, &store).unwrap();
        std::fs::write(&file, "class DemoGadget {}").unwrap();

        let data = tempfile::tempdir().unwrap();
        let services = Arc::new(
            AppServices::open(ummp_core::app::Settings::new(data.path().to_path_buf())).unwrap(),
        );
        let server = McpServer::new(services, vec![allowed.path().to_path_buf()]);
        let call = |confirm| {
            serde_json::to_value(
                server
                    .ummp_rollback(Parameters(RollbackArgs {
                        project: project.clone(),
                        checkpoint: None,
                        confirm,
                    }))
                    .unwrap(),
            )
            .unwrap()
        };

        let refused = call(false);
        assert_eq!(refused["isError"], true);
        assert!(refused.to_string().contains("confirm=true"));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "class DemoGadget {}",
            "confirm=false must not restore the checkpoint"
        );

        let restored = call(true);
        assert_ne!(restored["isError"], true, "{restored}");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "class DemoWidget {}"
        );
    }

    #[test]
    fn rollback_without_confirmation_is_refused_before_path_access() {
        let data = tempfile::tempdir().unwrap();
        let services = Arc::new(
            AppServices::open(ummp_core::app::Settings::new(data.path().to_path_buf())).unwrap(),
        );
        let server = McpServer::new(services, Vec::new());
        let response = server
            .ummp_rollback(Parameters(RollbackArgs {
                project: PathBuf::from("not-accessed"),
                checkpoint: None,
                confirm: false,
            }))
            .unwrap();
        let value = serde_json::to_value(response).unwrap();
        assert!(value["isError"].as_bool().unwrap());
    }

    #[test]
    fn report_contains_job_identity_and_state() {
        let value = serde_json::json!({
            "id": Uuid::nil(),
            "session": Uuid::nil(),
            "step": "port",
            "state": "Succeeded",
            "progress": null,
            "log": [],
            "result": null,
            "error": null
        });
        let job: JobView = serde_json::from_value(value).unwrap();
        let report = report_markdown(&job);
        assert!(report.contains(&Uuid::nil().to_string()));
        assert!(report.contains("port"));
    }

    #[test]
    fn report_contains_only_job_view_data() {
        let value = serde_json::json!({
            "id": Uuid::nil(),
            "session": Uuid::nil(),
            "step": "scan",
            "state": "Succeeded",
            "progress": null,
            "log": ["scan complete"],
            "result": { "clean": true },
            "error": null
        });
        let job: JobView = serde_json::from_value(value).unwrap();
        let report = report_markdown(&job);
        assert!(!report.contains("credentials.json"));
        assert!(!report.contains("api_key"));
    }
}
