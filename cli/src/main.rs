use clap::{CommandFactory, Parser};
use clap_complete::generate;
mod args;
mod config;
mod exit;
mod output;

use args::{AiArgs, AiKeyCmd, BuildArgs, Cli, Commands, PackCmd};
use config::CliConfig;
use exit::{CliFailure, ExitCode};
use std::fmt::Write as _;
use std::io::{BufRead, IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(windows)]
use std::process::Stdio;
use std::sync::Arc;
use ummp_ai_providers::{AnthropicProvider, OpenAiCompatibleProvider};
use ummp_core::adapter::AdapterRegistry;
use ummp_core::ai::orchestrator::{AiSessionReport, ApplyTarget, Orchestrator};
use ummp_core::ai::{AiConfig, AiProvider, ApprovalGate, ApproveReviewed, AuditLog, DenyAll};
use ummp_core::analyzer::AnalyzerEngine;
use ummp_core::app::credentials::{
    default_data_dir, resolve_ai_key, AiKeyStore, AiProviderKind, FileKeyStore,
};
use ummp_core::app::helpers::{open_kb, pack_format, NoIndex};
use ummp_core::app::snapshots::{default_snapshots, load_store, save_store};
use ummp_core::app::{AppServices, BuildJobRequest, PackRequest, PlanRequest, PortRequest};
use ummp_core::build::local::default_workdir;
use ummp_core::bytecode::{remap_jar, ClassMap};
use ummp_core::filesystem::{FileSystem, LocalFileSystem};
use ummp_core::jobs::JobState;
use ummp_core::knowledge::KnowledgeBase;
use ummp_core::migration::{MigrationEngine, MigrationPlanner, MigrationRequest, RunOptions};
use ummp_core::modpack::analyze::{analyze_pack, AnalyzeOptions};
use ummp_core::modpack::graph::PackGraph;
use ummp_core::modpack::plan::{ModIndex, Side};
use ummp_core::modpack::run::{migrate_pack, parse_loader, MigrateOptions};
use ummp_core::modpack::{import_with, ImportOptions};
use ummp_core::project::{DefaultProjectManager, ProjectManager};
use uuid::Uuid;

macro_rules! println {
    ($($arg:tt)*) => {{
        if output::is_json_mode() {
            std::eprintln!($($arg)*);
        } else {
            std::println!($($arg)*);
        }
    }};
}

macro_rules! print {
    ($($arg:tt)*) => {{
        if output::is_json_mode() {
            std::eprint!($($arg)*);
        } else {
            std::print!($($arg)*);
        }
    }};
}

/// Runs the pipeline and prints the short summary; returns the exit code.
fn run_pack_migrate(
    opts: &MigrateOptions,
    kb: &KnowledgeBase,
) -> Result<i32, Box<dyn std::error::Error>> {
    let index: Box<dyn ummp_core::modpack::plan::ModIndex> = if opts.online {
        Box::new(ummp_ai_providers::ModrinthIndex::new())
    } else {
        Box::new(NoIndex)
    };
    let out = migrate_pack(opts, index.as_ref(), kb)?;
    if output::is_json_mode() {
        output::set_result(serde_json::json!({
            "output": out.output,
            "report": out.report,
            "written": out.written,
            "exit_code": out.exit_code(),
        }));
    }
    let r = &out.report;
    println!("Output: {}", out.output.display());
    println!(
        "Mods: {} keep, {} update, {} port, {} replace, {} remove, {} manual",
        r.count("keep"),
        r.count("update"),
        r.count("port"),
        r.count("replace"),
        r.count("remove"),
        r.count("manual")
    );
    if let Some(why) = &out.plan.refused {
        eprintln!(
            "Refused: {why}\nReview the findings, then run again with --allow-risk to continue."
        );
    }
    for w in &out.written {
        println!("Wrote: {}", w.display());
    }
    let code = out.exit_code();
    Ok(if code == ExitCode::Partial.value() {
        ExitCode::Partial.value()
    } else {
        code
    })
}

/// Asks on the terminal for each proposed change.
struct PromptGate;

impl ApprovalGate for PromptGate {
    fn approve(&mut self, path: &Path, diff: &str, reviewer_ok: bool, concerns: &[String]) -> bool {
        println!("\n{diff}");
        println!(
            "Review agent: {}",
            if reviewer_ok {
                "approves"
            } else {
                "does NOT approve"
            }
        );
        for c in concerns {
            println!("  - {c}");
        }
        print!("Apply this change to {}? [y/N] ", path.display());
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).is_ok()
            && matches!(line.trim(), "y" | "Y" | "yes")
    }
}

fn provider(
    args: &AiArgs,
    config: &CliConfig,
) -> Result<Box<dyn AiProvider>, Box<dyn std::error::Error>> {
    let name = config.provider(args.provider.clone());
    let kind = match name.as_str() {
        "anthropic" => AiProviderKind::Anthropic,
        "openai" => AiProviderKind::Openai,
        other => {
            return Err(CliFailure::new(
                ExitCode::InvalidInput,
                format!("unknown provider '{other}'"),
            )
            .into());
        }
    };
    let data_dir = default_data_dir()?;
    let key = resolve_ai_key(&data_dir, kind)?.ok_or_else(|| {
        CliFailure::new(
            ExitCode::Refused,
            format!("No {name} API key is saved. Run `ummp ai-key set --provider {name}` or enter it in the desktop AI screen."),
        )
    })?;
    if kind == AiProviderKind::Anthropic {
        return Ok(Box::new(AnthropicProvider::with_api_key(
            key.expose().to_owned(),
            "https://api.anthropic.com",
        )));
    }
    let store = FileKeyStore::in_dir(&data_dir);
    let url = config
        .openai_base_url(args.base_url.clone())
        .or(store.openai_base_url()?)
        .unwrap_or_else(|| "https://api.openai.com/v1".into());
    Ok(Box::new(OpenAiCompatibleProvider::with_api_key(
        &url,
        key.expose().to_owned(),
    )))
}

fn open_configured_kb(
    kb: Option<PathBuf>,
    mappings: Option<PathBuf>,
    config: &CliConfig,
) -> Result<KnowledgeBase, Box<dyn std::error::Error>> {
    if kb.is_none() && mappings.is_none() {
        Ok(open_kb(config.kb_path(None), None)?)
    } else {
        Ok(open_kb(kb, mappings)?)
    }
}

fn gate(args: &AiArgs) -> Box<dyn ApprovalGate> {
    if output::is_json_mode() {
        Box::new(DenyAll)
    } else if args.approve_reviewed {
        Box::new(ApproveReviewed)
    } else if std::io::stdin().is_terminal() {
        Box::new(PromptGate)
    } else {
        eprintln!("No terminal to ask for approval: proposals are shown, nothing is applied.");
        Box::new(DenyAll)
    }
}

fn print_ai_report(r: &AiSessionReport, cfg: &AiConfig) {
    if let Some(a) = &r.analysis {
        println!("\n## Analysis\n{}", a.summary);
        for risk in &a.risks {
            println!("- [{}] {}: {}", risk.severity, risk.area, risk.detail);
        }
    }
    for n in &r.notes {
        println!("note: {n}");
    }
    for x in &r.rejected {
        println!("rejected {}: {}", x.path, x.reason);
    }
    for o in &r.omitted_context {
        println!("not sent to the model (too large): {o}");
    }
    println!(
        "\nProposed: {}, applied: {}",
        r.proposed.len(),
        r.applied.len()
    );
    if let Some(id) = r.checkpoint_id {
        println!("Checkpoint: {id} (ummp rollback restores it)");
    }
    print!("Tokens: {}", r.cost.total_tokens());
    match r.cost.cost_usd(&cfg.prices) {
        Some(usd) => println!(" (~${usd:.4})"),
        None => println!(" (set prices in --ai-config to see cost)"),
    }
}

fn ai_config(args: &AiArgs) -> Result<AiConfig, Box<dyn std::error::Error>> {
    Ok(match &args.ai_config {
        Some(p) => ummp_core::config::load_any(p)?,
        None => AiConfig::default(),
    })
}

/// Runs an application job to completion, forwarding its log to the CLI stream.
fn wait_for_job(
    services: &AppServices,
    id: Uuid,
    failure_code: ExitCode,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut logged = 0;
    loop {
        let job = services.job(id).map_err(|error| map_app_error(&error))?;
        for line in job.log.iter().skip(logged) {
            println!("{line}");
        }
        logged = job.log.len();
        match job.state {
            JobState::Succeeded => {
                return Ok(job.result.unwrap_or_else(|| serde_json::json!({})));
            }
            JobState::Failed | JobState::TimedOut => {
                let message = job.error.unwrap_or_else(|| "application job failed".into());
                let code = if message.starts_with("pack refused because of risk:") {
                    ExitCode::Refused
                } else {
                    failure_code
                };
                return Err(CliFailure::new(code, message).into());
            }
            JobState::Cancelled | JobState::Skipped => {
                return Err(
                    CliFailure::new(ExitCode::Error, "application job did not complete").into(),
                );
            }
            JobState::Pending | JobState::Running => {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
    }
}

/// Starts a build through AppServices, preserving the local-build risk confirmation.
fn run_app_build(
    path: &Path,
    args: BuildArgs,
    config: &CliConfig,
) -> Result<(bool, serde_json::Value), Box<dyn std::error::Error>> {
    let BuildArgs {
        local,
        repo,
        workdir,
        stages,
        timeout,
        java,
        i_accept_risk,
        yes,
        report,
    } = args;
    let services = app_services(config)?;
    let mut confirm_medium = yes || i_accept_risk;

    if local && !confirm_medium {
        let scan = services.scan(path).map_err(|error| map_app_error(&error))?;
        if scan.level == Some(ummp_core::app::views::RiskLevel::Medium) {
            if !std::io::stdin().is_terminal() {
                return Err(CliFailure::new(
                    ExitCode::InvalidInput,
                    "Medium risk: pass --yes to build on this machine",
                )
                .into());
            }
            print!("Medium risk. Build on this machine anyway? [y/N] ");
            std::io::stdout().flush()?;
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            if !line.trim().eq_ignore_ascii_case("y") {
                return Err(CliFailure::new(ExitCode::Refused, "build cancelled").into());
            }
            confirm_medium = true;
        }
    }

    let id = services
        .start_build(BuildJobRequest {
            path: path.to_path_buf(),
            local,
            repo: config.sandbox_repo(repo),
            stages,
            timeout_secs: timeout,
            java,
            workdir: Some(workdir.unwrap_or_else(default_workdir)),
            accept_high_risk: i_accept_risk,
            confirm_medium,
        })
        .map_err(|error| map_app_error(&error))?;
    let result = wait_for_job(&services, id, ExitCode::BuildFailed)?;

    if let (Some(path), Some(markdown)) = (report, result["report"].as_str()) {
        std::fs::write(path, markdown)?;
    }
    let passed = result["passed"].as_bool().unwrap_or(false);
    if let Some(markdown) = result["report"].as_str() {
        println!("{markdown}");
    }
    Ok((passed, result))
}

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let json_requested = arguments.iter().any(|arg| arg == "--json");
    let ai_key_set = arguments.windows(2).any(|pair| pair == ["ai-key", "set"]);
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let code = if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                ExitCode::Ok.value()
            } else {
                ExitCode::InvalidInput.value()
            };
            if ai_key_set {
                let message = "invalid ai-key set arguments; provide the key through hidden input or stdin, never as a command-line argument";
                if json_requested {
                    output::failure("invalid_input", message);
                } else {
                    eprintln!("error: {message}");
                }
            } else if json_requested {
                output::failure("invalid_input", &error.to_string());
            } else {
                let _ = error.print();
            }
            std::process::exit(code);
        }
    };
    let json_mode = cli.json;
    let command = command_name(&cli.command);
    output::set_json_mode(json_mode);
    let code = match run(cli) {
        Ok(code) => {
            if json_mode {
                output::success(command, code);
            }
            code
        }
        Err(error) => {
            let failure = error.downcast_ref::<CliFailure>();
            let app_error = error.downcast_ref::<ummp_core::app::AppError>();
            let code = failure
                .map(|e| e.code.value())
                .or_else(|| app_error.map(|e| map_app_error(e).code.value()))
                .unwrap_or(ExitCode::Error.value());
            if json_mode {
                let kind = match code {
                    2 => "refused",
                    4 => "build_failed",
                    5 => "invalid_input",
                    _ => "error",
                };
                output::failure(kind, &error.to_string());
            } else {
                eprintln!("error: {error}");
            }
            code
        }
    };
    std::process::exit(code);
}

fn run_pack(cmd: PackCmd, config: &CliConfig) -> Result<i32, Box<dyn std::error::Error>> {
    match cmd {
        PackCmd::Inspect {
            pack,
            from_loader,
            from_mc,
        } => {
            let work = tempfile::tempdir()?;
            let mut p = import_with(
                &pack,
                work.path(),
                &ImportOptions {
                    from_mc,
                    from_loader: from_loader.as_deref().map(parse_loader).transpose()?,
                    ..ImportOptions::default()
                },
            )?;
            analyze_pack(&mut p, &AnalyzeOptions { threads: 4 })?;
            let g = PackGraph::build(&p.entries);
            if output::is_json_mode() {
                output::set_result(serde_json::json!({
                    "pack": &p,
                    "issues": g.issues.iter().map(|issue| format!("{issue:?}")).collect::<Vec<_>>(),
                }));
            }
            println!(
                "{} ({:?}): Minecraft {}, loader {}",
                p.name,
                p.format,
                p.mc.as_deref().unwrap_or("?"),
                p.loader
                    .as_ref()
                    .map_or_else(|| "?".to_string(), |l| format!("{l:?}"))
            );
            for e in &p.entries {
                let risk = e.risk.map_or(String::new(), |r| format!(" risk={r:?}"));
                println!("  {} [{}]{risk}", e.path, e.display_id());
            }
            for w in &p.warnings {
                println!("warning: {w}");
            }
            for i in &g.issues {
                println!("issue: {i:?}");
            }
            Ok(0)
        }
        PackCmd::Migrate {
            pack,
            to,
            loader,
            from_loader,
            from_mc,
            online,
            sources,
            side: s,
            output,
            allow_risk,
            loader_version,
            format,
            kb,
            mappings,
            min_confidence,
        } => {
            let services = app_services(config)?;
            let index: Arc<dyn ModIndex + Send + Sync> = if online {
                Arc::new(ummp_ai_providers::ModrinthIndex::new())
            } else {
                Arc::new(NoIndex)
            };
            let id = services
                .start_pack(
                    PackRequest {
                        input: pack,
                        to,
                        loader,
                        from_loader,
                        from_mc,
                        online,
                        sources,
                        side: Some(s),
                        output,
                        allow_risk,
                        loader_version,
                        formats: format,
                        min_confidence,
                        kb: config.kb_path(kb),
                        mappings,
                    },
                    index,
                )
                .map_err(|error| map_app_error(&error))?;
            let result = wait_for_job(&services, id, ExitCode::Error)?;
            let code = i32::try_from(result["exit_code"].as_i64().unwrap_or(0)).unwrap_or(0);
            println!("Output: {}", result["output"].as_str().unwrap_or("?"));
            println!("Manual items: {}", result["manual"].as_u64().unwrap_or(0));
            if output::is_json_mode() {
                output::set_result(result);
            }
            Ok(code)
        }
        PackCmd::Export {
            pack,
            to_format,
            output,
        } => {
            let work = tempfile::tempdir()?;
            let p = import_with(&pack, work.path(), &ImportOptions::default())?;
            let (Some(mc), Some(loader)) = (p.mc.clone(), p.loader) else {
                return Err(CliFailure::new(
                    ExitCode::InvalidInput,
                    "cannot tell the pack's Minecraft version or loader; use `pack migrate`",
                )
                .into());
            };
            let kb = KnowledgeBase::open(Path::new(":memory:"))?;
            run_pack_migrate(
                &MigrateOptions {
                    input: pack,
                    to: mc,
                    loader: format!("{loader:?}").to_lowercase(),
                    from_loader: None,
                    from_mc: None,
                    online: false,
                    sources: None,
                    side: Side::Both,
                    output: config.output_dir(output),
                    allow_risk: false,
                    loader_version: None,
                    formats: vec![pack_format(&to_format)?],
                    threads: 4,
                    min_confidence: 0.8,
                },
                &kb,
            )
        }
    }
}

fn parse_ai_provider(value: &str) -> Result<AiProviderKind, CliFailure> {
    match value {
        "anthropic" => Ok(AiProviderKind::Anthropic),
        "openai" => Ok(AiProviderKind::Openai),
        _ => Err(CliFailure::new(
            ExitCode::InvalidInput,
            "provider must be anthropic or openai",
        )),
    }
}

#[cfg(unix)]
struct TerminalEchoGuard;

#[cfg(unix)]
impl Drop for TerminalEchoGuard {
    fn drop(&mut self) {
        let _ = Command::new("stty").arg("echo").status();
        eprintln!();
    }
}

fn read_api_key() -> Result<String, Box<dyn std::error::Error>> {
    if !std::io::stdin().is_terminal() {
        let mut value = String::new();
        std::io::stdin().lock().read_line(&mut value)?;
        return Ok(value.trim().to_owned());
    }

    eprint!("API key (input hidden): ");
    std::io::stderr().flush()?;
    #[cfg(unix)]
    {
        let status = Command::new("stty").arg("-echo").status()?;
        if !status.success() {
            return Err("could not disable terminal echo".into());
        }
        let _echo_guard = TerminalEchoGuard;
        let mut value = String::new();
        std::io::stdin().lock().read_line(&mut value)?;
        Ok(value.trim().to_owned())
    }
    #[cfg(windows)]
    {
        let script = "$s=Read-Host 'API key' -AsSecureString; $p=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($s); try {[Runtime.InteropServices.Marshal]::PtrToStringBSTR($p)} finally {[Runtime.InteropServices.Marshal]::ZeroFreeBSTR($p)}";
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", script])
            .stdin(Stdio::inherit())
            .output()?;
        if !output.status.success() {
            return Err("could not read the API key securely".into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(
            "hidden terminal input is not supported on this platform; pipe the key through stdin"
                .into(),
        )
    }
}

fn run_ai_key(cmd: AiKeyCmd, json_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = default_data_dir()?;
    let store = FileKeyStore::in_dir(&data_dir);
    match cmd {
        AiKeyCmd::Set { provider, base_url } => {
            let kind = parse_ai_provider(&provider)?;
            if kind == AiProviderKind::Openai {
                ummp_core::app::credentials::validate_openai_base_url(base_url.as_deref())?;
            }
            let key = read_api_key()?;
            let warning = store.set(kind, &ummp_core::app::Secret::new(key))?;
            if kind == AiProviderKind::Openai {
                if let Some(url) = base_url.as_deref() {
                    store.set_openai_base_url(Some(url))?;
                }
            }
            let result = serde_json::json!({
                "provider": provider,
                "saved": true,
                "masked": store.masked(kind)?,
                "warning": warning,
            });
            if json_mode {
                output::set_result(result);
            } else {
                println!(
                    "Saved {} API key: {}",
                    provider,
                    result["masked"].as_str().unwrap_or("saved")
                );
                if let Some(warning) = warning {
                    eprintln!("Warning: {warning}");
                }
            }
        }
        AiKeyCmd::Status { provider } => {
            let providers = match provider {
                None => vec![
                    ("anthropic", AiProviderKind::Anthropic),
                    ("openai", AiProviderKind::Openai),
                ],
                Some(name) => {
                    let kind = parse_ai_provider(&name)?;
                    vec![(
                        if kind == AiProviderKind::Anthropic {
                            "anthropic"
                        } else {
                            "openai"
                        },
                        kind,
                    )]
                }
            };
            let entries = providers
                .into_iter()
                .map(|(name, kind)| {
                    let saved = store.get(kind)?.is_some();
                    let masked = store.masked(kind)?;
                    Ok(serde_json::json!({
                        "provider": name,
                        "saved": saved,
                        "masked": masked,
                    }))
                })
                .collect::<Result<Vec<_>, ummp_core::app::AppError>>()?;
            let result = serde_json::json!({
                "path": data_dir.join("credentials.json"),
                "providers": entries,
            });
            if json_mode {
                output::set_result(result);
            } else {
                println!(
                    "Credentials: {}",
                    result["path"].as_str().unwrap_or_default()
                );
                for entry in result["providers"].as_array().into_iter().flatten() {
                    println!(
                        "{}: {}",
                        entry["provider"].as_str().unwrap_or_default(),
                        entry["masked"].as_str().unwrap_or("not saved")
                    );
                }
            }
        }
        AiKeyCmd::Delete { provider } => {
            let providers = match provider {
                None => vec![AiProviderKind::Anthropic, AiProviderKind::Openai],
                Some(name) => vec![parse_ai_provider(&name)?],
            };
            for kind in providers {
                store.delete(kind)?;
                if kind == AiProviderKind::Openai {
                    store.set_openai_base_url(None)?;
                }
            }
            let result = serde_json::json!({ "deleted": true });
            if json_mode {
                output::set_result(result);
            } else {
                println!("Deleted selected AI credential(s).");
            }
        }
    }
    Ok(())
}

/// Formats the application plan view for the human-readable CLI.
fn plan_markdown(plan: &ummp_core::app::views::PlanView) -> String {
    let mut out = format!(
        "# Migration plan\n\nMinecraft {} -> {} | loader {} -> {}\n\n{} files scanned, {} files changed\n",
        plan.source_minecraft,
        plan.target_minecraft,
        plan.source_loader.as_deref().unwrap_or("unknown"),
        plan.target_loader,
        plan.files_scanned,
        plan.files.len()
    );
    for file in &plan.files {
        let _ = writeln!(out, "\n## {} ({:?})\n\n{}", file.path, file.kind, file.diff);
        if file.needs_attention {
            out.push_str("Needs adapter/manual attention.\n");
        }
    }
    for warning in &plan.warnings {
        let _ = writeln!(out, "\nWarning: {warning}");
    }
    for skipped in &plan.skipped {
        let _ = writeln!(
            out,
            "\nSkipped: {} ({})",
            skipped.description, skipped.reason
        );
    }
    out
}

fn map_app_error(error: &ummp_core::app::AppError) -> CliFailure {
    let code = match &error {
        ummp_core::app::AppError::Input(_) => ExitCode::InvalidInput,
        ummp_core::app::AppError::Refused(_) => ExitCode::Refused,
        _ => ExitCode::Error,
    };
    CliFailure::new(code, error.to_string())
}

fn app_services(
    config: &CliConfig,
) -> Result<ummp_core::app::AppServices, Box<dyn std::error::Error>> {
    use ummp_core::app::{AppServices, Settings};

    let mut settings = Settings::new(config::data_dir());
    settings.kb_path.clone_from(&config.kb);
    settings.output_root.clone_from(&config.output_root);
    settings.sandbox_repo.clone_from(&config.sandbox_repo);
    settings.ai_provider.clone_from(&config.ai_provider);
    AppServices::open(settings)
        .map_err(|error| Box::new(map_app_error(&error)) as Box<dyn std::error::Error>)
}

const fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::AiKey { .. } => "ai-key",
        Commands::Completions { .. } => "completions",
        Commands::Mcp => "mcp",
        Commands::Serve { .. } => "serve",
        Commands::Analyze { .. } => "analyze",
        Commands::Migrate { .. } => "migrate",
        Commands::RemapJar { .. } => "remap-jar",
        Commands::AiAssist { .. } => "ai-assist",
        Commands::AiDebug { .. } => "ai-debug",
        Commands::Scan { .. } => "scan",
        Commands::Build { .. } => "build",
        Commands::Port { .. } => "port",
        Commands::Adapters => "adapters",
        Commands::Pack { .. } => "pack",
        Commands::Rollback { .. } => "rollback",
    }
}

fn run(cli: Cli) -> Result<i32, Box<dyn std::error::Error>> {
    let Cli {
        json: json_mode,
        config: config_path,
        command,
    } = cli;
    let config = if matches!(
        &command,
        Commands::Completions { .. } | Commands::AiKey { .. }
    ) {
        CliConfig::default()
    } else {
        config_path
            .as_deref()
            .map_or_else(CliConfig::load, |path| {
                CliConfig::load_with_path(Some(path))
            })
            .map_err(|error| {
                CliFailure::new(
                    ExitCode::InvalidInput,
                    format!("invalid configuration: {error}"),
                )
            })?
    };
    let pm = DefaultProjectManager;
    let mut exit_code = ExitCode::Ok.value();

    match command {
        Commands::AiKey { cmd } => run_ai_key(cmd, json_mode)?,
        Commands::Mcp => {
            if json_mode {
                return Err(CliFailure::new(
                    ExitCode::InvalidInput,
                    "--json cannot be combined with the stdio MCP server",
                )
                .into());
            }
            let services = Arc::new(app_services(&config)?);
            ummp_server::mcp::serve(services, config.allowed_roots())?;
        }
        Commands::Serve {
            listen,
            port,
            i_understand_remote,
        } => {
            if json_mode {
                return Err(CliFailure::new(
                    ExitCode::InvalidInput,
                    "--json cannot be combined with the long-running REST server",
                )
                .into());
            }
            let services = Arc::new(app_services(&config)?);
            ummp_server::serve(
                services,
                ummp_server::ServerConfig {
                    data_dir: config::data_dir(),
                    allowed_roots: config.allowed_roots(),
                    listen,
                    port,
                    allow_remote: i_understand_remote,
                },
            )?;
        }
        Commands::Completions { shell } => {
            if json_mode {
                return Err(CliFailure::new(
                    ExitCode::InvalidInput,
                    "--json cannot be combined with shell completion generation",
                )
                .into());
            }
            let mut command = Cli::command();
            generate(shell, &mut command, "ummp", &mut std::io::stdout());
        }
        Commands::Analyze { path } => {
            let report = app_services(&config)?
                .analyze(&path)
                .map_err(|error| map_app_error(&error))?;
            if json_mode {
                output::set_result(report);
            } else {
                println!("Input: {}", path.display());
                for m in &report.mods {
                    println!(
                        "{} ({}) v{} | {} | Minecraft {} | Java {:?}",
                        m.name, m.id, m.version, m.loader, m.minecraft, m.java
                    );
                }
                if let Some(build) = report.build_system {
                    println!("Build system: {build}");
                }
                if !report.languages.is_empty() {
                    println!("Languages: {}", report.languages.join(", "));
                }
                for tech in &report.technologies {
                    println!("Technology: {tech:?}");
                }
            }
        }

        Commands::Migrate {
            path,
            from,
            to,
            loader,
            kb,
            mappings,
            min_confidence,
            from_loader,
            apply,
            snapshots,
            report,
        } => {
            let services = app_services(&config)?;
            let request = PlanRequest {
                input: path,
                from: Some(from),
                to,
                loader: Some(loader.to_lowercase()),
                min_confidence,
                source_loader: from_loader.map(|value| value.to_lowercase()),
                kb,
                mappings,
                snapshots,
            };
            let plan = services
                .plan(&request)
                .map_err(|error| map_app_error(&error))?;
            let markdown = plan_markdown(&plan);
            if apply && !json_mode {
                print!("{markdown}");
            }
            let result = if apply {
                let id = services
                    .start_migrate_in_place(request, true)
                    .map_err(|error| map_app_error(&error))?;
                let migration = wait_for_job(&services, id, ExitCode::Error)?;
                serde_json::json!({ "plan": plan, "migration": migration })
            } else {
                serde_json::to_value(&plan)?
            };
            if let Some(out) = report {
                let body = if out.extension().is_some_and(|e| e == "json") {
                    serde_json::to_string_pretty(&result)?
                } else {
                    markdown.clone()
                };
                std::fs::write(&out, body)?;
            }
            if json_mode {
                output::set_result(result);
            } else if apply {
                println!(
                    "\nMigration completed. Checkpoint: {}",
                    result["migration"]["checkpoint"]
                        .as_str()
                        .unwrap_or("created")
                );
            } else {
                print!("{markdown}");
                if !plan.files.is_empty() {
                    eprintln!("\nDry run: nothing was written. Re-run with --apply to write.");
                }
            }
        }

        Commands::RemapJar {
            input,
            output,
            from,
            to,
            loader,
            kb,
            mappings,
            min_confidence,
        } => {
            let kb = open_configured_kb(kb, mappings, &config)?;
            let request = MigrationRequest {
                source_minecraft: from,
                target_minecraft: to,
                target_loader: loader.to_lowercase(),
                min_confidence,
                source_loader: None,
            };
            let plan = MigrationPlanner::plan(&kb, &request)?;
            let stages: Vec<ClassMap> = plan
                .stages
                .iter()
                .map(|s| ClassMap::from_rules(&s.rules))
                .filter(|m| !m.is_empty())
                .collect();
            if stages.is_empty() {
                return Err(CliFailure::new(
                    ExitCode::InvalidInput,
                    "no qualified class mappings for this request",
                )
                .into());
            }
            let stats = remap_jar(&input, &output, &stages)?;
            let result = serde_json::json!({
                "output": output,
                "classes_remapped": stats.classes_changed,
                "class_files_moved": stats.entries_moved,
                "signature_files_dropped": stats.signatures_dropped,
            });
            if json_mode {
                output::set_result(result);
            } else {
                println!(
                    "Wrote {}: {} classes remapped, {} class files moved, {} signature files dropped",
                    output.display(),
                    stats.classes_changed,
                    stats.entries_moved,
                    stats.signatures_dropped
                );
            }
        }

        Commands::AiAssist {
            path,
            from,
            to,
            loader,
            kb,
            mappings,
            ai,
        } => {
            let project = pm.open(&path)?;
            let kb = open_configured_kb(kb, mappings, &config)?;
            let request = MigrationRequest {
                source_minecraft: from.clone(),
                target_minecraft: to.clone(),
                target_loader: loader.to_lowercase(),
                min_confidence: 0.8,
                source_loader: None,
            };
            let plan = MigrationPlanner::plan(&kb, &request)?;
            let snap_dir = ai
                .snapshots
                .clone()
                .unwrap_or_else(|| default_snapshots(&project.root));
            let mut store = load_store(&snap_dir)?;
            let engine = MigrationEngine::default();
            // Rule-based dry run: its report and warnings are the agents' input.
            let rules = engine.run(
                &plan,
                &project,
                &mut store,
                &RunOptions {
                    dry_run: true,
                    snapshot_root: &snap_dir,
                    session_id: Uuid::new_v4(),
                },
            )?;
            let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
            let analysis = AnalyzerEngine::default().analyze(&project.root, &fs)?;
            let cfg = ai_config(&ai)?;
            let provider = provider(&ai, &config)?;
            let mut gate = gate(&ai);
            let mut orch = Orchestrator::new(
                provider.as_ref(),
                cfg.clone(),
                AuditLog::new(Some(&snap_dir.join("ai-audit.jsonl"))),
            );
            let report = orch.assist(
                &format!("Minecraft {from} -> {to} on {loader}"),
                &analysis.to_text(),
                &rules.to_markdown(),
                &rules.warnings,
                gate.as_mut(),
                &mut ApplyTarget {
                    project: &project,
                    engine: &engine,
                    store: &mut store,
                    snapshot_root: &snap_dir,
                },
            )?;
            if report.checkpoint_id.is_some() {
                save_store(&snap_dir, &store)?;
            }
            if json_mode {
                output::set_result(&report);
            }
            print_ai_report(&report, &cfg);
        }

        Commands::AiDebug {
            path,
            log,
            from,
            to,
            loader,
            ai,
        } => {
            let project = pm.open(&path)?;
            let log_text = std::fs::read_to_string(&log)?;
            let snap_dir = ai
                .snapshots
                .clone()
                .unwrap_or_else(|| default_snapshots(&project.root));
            let mut store = load_store(&snap_dir)?;
            let engine = MigrationEngine::default();
            let cfg = ai_config(&ai)?;
            let provider = provider(&ai, &config)?;
            let mut gate = gate(&ai);
            let mut orch = Orchestrator::new(
                provider.as_ref(),
                cfg.clone(),
                AuditLog::new(Some(&snap_dir.join("ai-audit.jsonl"))),
            );
            let report = orch.debug(
                &format!("Minecraft {from} -> {to} on {loader}"),
                &log_text,
                gate.as_mut(),
                &mut ApplyTarget {
                    project: &project,
                    engine: &engine,
                    store: &mut store,
                    snapshot_root: &snap_dir,
                },
            )?;
            if report.checkpoint_id.is_some() {
                save_store(&snap_dir, &store)?;
            }
            if json_mode {
                output::set_result(&report);
            }
            print_ai_report(&report, &cfg);
        }

        Commands::Scan { path } => {
            let report = app_services(&config)?
                .scan(&path)
                .map_err(|error| map_app_error(&error))?;
            if report.level == Some(ummp_core::app::views::RiskLevel::High) {
                exit_code = ExitCode::Refused.value();
            }
            if json_mode {
                output::set_result(report);
            } else {
                println!(
                    "Risk: {:?}",
                    report
                        .level
                        .unwrap_or(ummp_core::app::views::RiskLevel::Low)
                );
                for finding in &report.findings {
                    println!(
                        "[{:?}] {}: {} ({})",
                        finding.severity, finding.rule, finding.evidence, finding.file
                    );
                }
            }
        }

        Commands::Build { path, opts } => {
            let (passed, result) = run_app_build(&path, opts, &config)?;
            if !passed {
                exit_code = ExitCode::BuildFailed.value();
            }
            if json_mode {
                output::set_result(result);
            }
        }

        Commands::Port {
            input,
            to,
            loader,
            from,
            output,
            zip,
            kb,
            mappings,
            min_confidence,
            build,
            build_opts,
        } => {
            let services = app_services(&config)?;
            let id = services
                .start_port(PortRequest {
                    input,
                    output: config.output_dir(output),
                    to,
                    from,
                    loader,
                    zip,
                    min_confidence,
                    kb,
                    mappings,
                })
                .map_err(|error| map_app_error(&error))?;
            let port_result = wait_for_job(&services, id, ExitCode::Error)?;
            println!(
                "Ported source: {}",
                port_result["output"].as_str().unwrap_or("?")
            );
            if let Some(zip) = port_result["zip"].as_str() {
                println!("Zip: {zip}");
            }
            println!(
                "Report: {}/UMMP_REPORT.md",
                port_result["output"].as_str().unwrap_or("?")
            );

            let result = if build {
                let output = PathBuf::from(port_result["output"].as_str().ok_or_else(|| {
                    CliFailure::new(ExitCode::Error, "port job returned no output path")
                })?);
                let (passed, build_result) = run_app_build(&output, build_opts, &config)?;
                if !passed {
                    exit_code = ExitCode::BuildFailed.value();
                }
                serde_json::json!({ "port": port_result, "build": build_result })
            } else {
                port_result
            };
            if json_mode {
                output::set_result(result);
            }
        }

        Commands::Pack { cmd } => {
            exit_code = run_pack(cmd, &config)?;
        }

        Commands::Adapters => {
            let adapters = AdapterRegistry::default().list();
            if json_mode {
                output::set_result(serde_json::json!({
                    "adapters": adapters.iter().map(|(id, level)| {
                        serde_json::json!({ "id": id, "level": level })
                    }).collect::<Vec<_>>(),
                }));
            } else {
                println!("{:<20} level", "adapter");
                for (id, level) in adapters {
                    println!("{id:<20} {level:?}");
                }
            }
        }

        Commands::Rollback {
            path,
            snapshots,
            checkpoint,
        } => {
            let services = app_services(&config)?;
            let id = services
                .rollback_with_snapshots(&path, checkpoint, snapshots.as_deref())
                .map_err(|error| map_app_error(&error))?;
            let result = serde_json::json!({
                "restored": true,
                "project": path,
                "checkpoint": id,
            });
            if json_mode {
                output::set_result(result);
            } else {
                println!("Restored {} to checkpoint {id}", path.display());
            }
        }
    }

    Ok(exit_code)
}
