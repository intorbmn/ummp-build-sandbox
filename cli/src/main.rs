use clap::{ArgGroup, Parser, Subcommand};
use std::io::{BufRead, IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ummp_ai_providers::{AnthropicProvider, OpenAiCompatibleProvider};
use ummp_core::adapter::{AdapterRegistry, Handling};
use ummp_core::ai::orchestrator::{AiSessionReport, ApplyTarget, Orchestrator};
use ummp_core::ai::{AiConfig, AiProvider, ApprovalGate, ApproveReviewed, AuditLog, DenyAll};
use ummp_core::analyzer::AnalyzerEngine;
use ummp_core::app::helpers::{
    detected_loader, mod_identity, open_kb, pack_format, risk_decision, side, NoIndex, RiskDecision,
};
use ummp_core::app::snapshots::{default_snapshots, load_store, save_store};
use ummp_core::build::github::GithubBackend;
use ummp_core::build::local::{default_workdir, LocalBackend};
use ummp_core::build::report::{score, to_markdown};
use ummp_core::build::{BuildBackend, BuildRequest, Stage};
use ummp_core::bytecode::{remap_jar, ClassMap};
use ummp_core::filesystem::{FileSystem, LocalFileSystem};
use ummp_core::knowledge::KnowledgeBase;
use ummp_core::migration::{MigrationEngine, MigrationPlanner, MigrationRequest, RunOptions};
use ummp_core::modpack::analyze::{analyze_pack, AnalyzeOptions};
use ummp_core::modpack::graph::PackGraph;
use ummp_core::modpack::plan::Side;
use ummp_core::modpack::run::{migrate_pack, parse_loader, MigrateOptions};
use ummp_core::modpack::{import_with, ImportOptions};
use ummp_core::port::{port, PortOptions};
use ummp_core::project::{DefaultProjectManager, ProjectManager};
use ummp_core::scan::{scan_path, Severity};
use uuid::Uuid;

#[derive(Subcommand)]
enum PackCmd {
    /// Read a pack (.mrpack, CurseForge .zip, Prism instance or folder) and report. Writes nothing.
    Inspect {
        pack: PathBuf,
        #[arg(long)]
        from_loader: Option<String>,
        #[arg(long)]
        from_mc: Option<String>,
    },
    /// Migrate a pack to another Minecraft version and/or loader.
    Migrate {
        pack: PathBuf,
        /// Target Minecraft version.
        #[arg(long)]
        to: String,
        /// Target loader: forge, fabric, neoforge or quilt.
        #[arg(long)]
        loader: String,
        #[arg(long)]
        from_loader: Option<String>,
        #[arg(long)]
        from_mc: Option<String>,
        /// Ask Modrinth (read-only) for versions of each mod that fit the target.
        #[arg(long)]
        online: bool,
        /// Folder with `<mod-id>/` or `<mod-id>.zip` sources for mods you want ported.
        #[arg(long)]
        sources: Option<PathBuf>,
        /// client, server or both.
        #[arg(long, default_value = "both")]
        side: String,
        /// Output folder (default: <packs root>/<pack>-<mc>-<loader>-<time>).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Continue even when a JAR is rated High risk (you reviewed it; the report keeps the rating).
        #[arg(long)]
        allow_risk: bool,
        #[arg(long)]
        loader_version: Option<String>,
        /// Comma list of mrpack,curseforge,prism,folder.
        #[arg(long, value_delimiter = ',', default_value = "mrpack,folder")]
        format: Vec<String>,
        /// Knowledge base for porting mods from `--sources`.
        #[arg(long)]
        kb: Option<PathBuf>,
        #[arg(long)]
        mappings: Option<PathBuf>,
        #[arg(long, default_value_t = 0.8)]
        min_confidence: f64,
    },
    /// Convert a pack to another format without changing versions.
    Export {
        pack: PathBuf,
        #[arg(long)]
        to_format: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
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
    Ok(out.exit_code())
}

#[derive(Parser)]
#[command(name = "ummp", version, about = "Universal Minecraft Mod Porter")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Detect loader, Minecraft/Java versions, dependencies and build system.
    Analyze {
        /// Project root path (source tree or a folder of JARs).
        path: PathBuf,
        /// Print the machine-readable JSON report instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Plan and run a source migration. Dry run unless `--apply` is given.
    #[command(group(ArgGroup::new("knowledge").required(true).args(["kb", "mappings"])))]
    Migrate {
        /// Project root path.
        path: PathBuf,
        /// Source Minecraft version, e.g. 1.16.5.
        #[arg(long)]
        from: String,
        /// Target Minecraft version, e.g. 1.17.1.
        #[arg(long)]
        to: String,
        /// Target loader: forge, fabric, neoforge, quilt.
        #[arg(long)]
        loader: String,
        /// Knowledge-base SQLite file (read only).
        #[arg(long)]
        kb: Option<PathBuf>,
        /// JSON file with a list of mappings (same format as KB export).
        #[arg(long)]
        mappings: Option<PathBuf>,
        /// Mappings below this confidence are reported, not applied.
        #[arg(long, default_value_t = 0.8)]
        min_confidence: f64,
        /// Loader the mod uses now (default: detected). A different `--loader` converts it.
        #[arg(long)]
        from_loader: Option<String>,
        /// Write the changes (a checkpoint is taken first).
        #[arg(long)]
        apply: bool,
        /// Checkpoint directory; defaults to `<project>.ummp-snapshots` next to the project.
        #[arg(long)]
        snapshots: Option<PathBuf>,
        /// Also write the report to this file (`.json` or Markdown).
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Remap class names inside a compiled mod JAR into a new JAR (input untouched).
    #[command(group(ArgGroup::new("knowledge").required(true).args(["kb", "mappings"])))]
    RemapJar {
        /// Input mod JAR.
        input: PathBuf,
        /// Output JAR; must not exist.
        output: PathBuf,
        /// Source Minecraft version.
        #[arg(long)]
        from: String,
        /// Target Minecraft version.
        #[arg(long)]
        to: String,
        /// Target loader.
        #[arg(long)]
        loader: String,
        /// Knowledge-base SQLite file (read only).
        #[arg(long)]
        kb: Option<PathBuf>,
        /// JSON file with a list of mappings.
        #[arg(long)]
        mappings: Option<PathBuf>,
        /// Mappings below this confidence are not applied.
        #[arg(long, default_value_t = 0.8)]
        min_confidence: f64,
    },
    /// AI review of a migration: analysis, proposed fixes, review, then your approval.
    #[command(group(ArgGroup::new("knowledge").required(true).args(["kb", "mappings"])))]
    AiAssist {
        /// Project root path.
        path: PathBuf,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        loader: String,
        #[arg(long)]
        kb: Option<PathBuf>,
        #[arg(long)]
        mappings: Option<PathBuf>,
        #[command(flatten)]
        ai: AiArgs,
    },
    /// AI fix proposals for a build or crash log, reviewed, then your approval.
    AiDebug {
        /// Project root path.
        path: PathBuf,
        /// Build or crash log file.
        #[arg(long)]
        log: PathBuf,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        loader: String,
        #[command(flatten)]
        ai: AiArgs,
    },
    /// Static malware-risk scan of a mod project or JAR. Exit code 2 when High risk.
    Scan {
        /// Project folder or JAR file.
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Build, test and start a dedicated server; on GitHub Actions unless `--local`.
    Build {
        /// Project root path (source tree with Gradle build files).
        path: PathBuf,
        #[command(flatten)]
        opts: BuildArgs,
    },
    /// Port a mod's source: input folder or .zip in, ported source out (input untouched).
    Port {
        /// Source folder or .zip of the mod.
        input: PathBuf,
        /// Target Minecraft version.
        #[arg(long)]
        to: String,
        /// Target loader (fabric, quilt, forge, neoforge); default: the mod's loader.
        #[arg(long)]
        loader: Option<String>,
        /// Source Minecraft version; default: detected from the mod descriptor.
        #[arg(long)]
        from: Option<String>,
        /// Output folder (default: D:\Claude\ummp-ports\<mod>-<mc>-<loader>-<time> on
        /// Windows, ~/ummp-ports/... elsewhere). Must be empty or missing.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Also write <output>.zip.
        #[arg(long)]
        zip: bool,
        /// Knowledge base SQLite file with API mappings.
        #[arg(long, conflicts_with = "mappings")]
        kb: Option<PathBuf>,
        /// Mappings JSON (knowledge-base export format).
        #[arg(long)]
        mappings: Option<PathBuf>,
        #[arg(long, default_value_t = 0.8)]
        min_confidence: f64,
        /// Build, test and start a server with the ported source (see `ummp build`).
        #[arg(long)]
        build: bool,
        #[command(flatten)]
        build_opts: BuildArgs,
    },
    /// List technology adapters and how far each one goes.
    Adapters,
    /// Modpacks: inspect, migrate to another Minecraft version/loader, convert between formats.
    /// Exit codes: 0 done, 2 refused (High-risk JAR, use --allow-risk), 3 done but some mods
    /// are left for you (see UMMP_PACK_REPORT.md).
    Pack {
        #[command(subcommand)]
        cmd: PackCmd,
    },
    /// Restore the project to a checkpoint taken by `migrate --apply`.
    Rollback {
        /// Project root path.
        path: PathBuf,
        /// Checkpoint directory used by `migrate`.
        #[arg(long)]
        snapshots: Option<PathBuf>,
        /// Checkpoint id; defaults to the latest one.
        #[arg(long)]
        checkpoint: Option<Uuid>,
    },
}

#[derive(clap::Args)]
struct BuildArgs {
    /// Build on this machine instead of GitHub.
    #[arg(long)]
    local: bool,
    /// GitHub build repository `owner/name` (default: $UMMP_BUILD_REPO or
    /// intorbmn/ummp-build-sandbox).
    #[arg(long)]
    repo: Option<String>,
    /// Folder for build copies (default: D:\Claude\ummp-builds on Windows, ~/ummp-builds).
    #[arg(long)]
    workdir: Option<PathBuf>,
    /// Comma-separated stages: build,test,server.
    #[arg(long, value_delimiter = ',', default_value = "build,test,server")]
    stages: Vec<String>,
    /// Limit per stage, in seconds.
    #[arg(long, default_value_t = 1800)]
    timeout: u64,
    /// Java major version (default: detected from the mod).
    #[arg(long)]
    java: Option<u32>,
    /// Build a High-risk mod on this machine anyway.
    #[arg(long)]
    i_accept_risk: bool,
    /// Build a Medium-risk mod on this machine without asking.
    #[arg(long)]
    yes: bool,
    /// Also write the Markdown report here.
    #[arg(long)]
    report: Option<PathBuf>,
}

#[derive(clap::Args)]
struct AiArgs {
    /// `anthropic` (ANTHROPIC_API_KEY) or `openai` (OpenAI-compatible, e.g. Ollama).
    #[arg(long, default_value = "anthropic")]
    provider: String,
    /// Base URL for `--provider openai`, e.g. http://localhost:11434/v1.
    #[arg(long)]
    base_url: Option<String>,
    /// YAML/JSON AI config: per-agent models, budget, retries, prices.
    #[arg(long)]
    ai_config: Option<PathBuf>,
    /// Apply every change the review agent approved without asking.
    /// Changes the reviewer rejected are never applied by this flag.
    #[arg(long)]
    approve_reviewed: bool,
    /// Checkpoint and audit directory.
    #[arg(long)]
    snapshots: Option<PathBuf>,
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

fn provider(args: &AiArgs) -> Result<Box<dyn AiProvider>, Box<dyn std::error::Error>> {
    Ok(match args.provider.as_str() {
        "anthropic" => Box::new(AnthropicProvider::from_env()?),
        "openai" => {
            let url = args
                .base_url
                .as_deref()
                .ok_or("--provider openai needs --base-url")?;
            Box::new(OpenAiCompatibleProvider::new(url))
        }
        other => return Err(format!("unknown provider '{other}'").into()),
    })
}

fn gate(args: &AiArgs) -> Box<dyn ApprovalGate> {
    if args.approve_reviewed {
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

/// Local builds run mod code on this machine: High risk needs `--i-accept-risk`, Medium
/// needs `--yes` or a y/N answer. GitHub builds run on a disposable runner and only warn.
fn check_risk(
    risk: Option<Severity>,
    local: bool,
    accept: bool,
    yes: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match risk_decision(risk, local, accept, yes) {
        RiskDecision::Allow => Ok(()),
        RiskDecision::AllowWithWarning => {
            println!("Warning: risk indicators found; building on a disposable GitHub runner.");
            Ok(())
        }
        RiskDecision::Refuse(why) => Err(why.into()),
        RiskDecision::Confirm => {
            if !std::io::stdin().is_terminal() {
                return Err("Medium risk: pass --yes to build on this machine".into());
            }
            print!("Medium risk. Build on this machine anyway? [y/N] ");
            std::io::stdout().flush()?;
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            if line.trim().eq_ignore_ascii_case("y") {
                Ok(())
            } else {
                Err("build cancelled".into())
            }
        }
    }
}

/// Runs `ummp build`; `Ok(true)` when every stage that ran passed.
fn run_build(path: &Path, a: BuildArgs) -> Result<bool, Box<dyn std::error::Error>> {
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
    } = a;
    let path = path.to_path_buf();
    if !path.is_dir() {
        return Err(
            "JAR-only mods can only be scanned (`ummp scan`); `build` needs the source project"
                .into(),
        );
    }
    let scan = scan_path(&path)?;
    print!("{}", scan.to_text());
    check_risk(scan.risk(), local, i_accept_risk, yes)?;
    let stages = stages
        .iter()
        .map(|s| match s.trim() {
            "build" => Ok(Stage::Build),
            "test" => Ok(Stage::Test),
            "server" => Ok(Stage::Server),
            other => Err(format!("unknown stage `{other}` (use build, test, server)")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (mod_name, detected_java) = mod_identity(&path);
    let req = BuildRequest {
        project: path,
        mod_name,
        stages,
        java: java.or(detected_java),
        timeout: std::time::Duration::from_secs(timeout),
    };
    let workdir = workdir.unwrap_or_else(default_workdir);
    let outcome = if local {
        LocalBackend {
            workdir,
            gradle: None,
        }
        .run(&req)?
    } else {
        let repo = repo
            .or_else(|| std::env::var("UMMP_BUILD_REPO").ok())
            .unwrap_or_else(|| "intorbmn/ummp-build-sandbox".into());
        println!("Building on GitHub Actions in {repo} (this takes a few minutes)...");
        GithubBackend {
            repo,
            gh: "gh".into(),
            git: "git".into(),
            poll: std::time::Duration::from_secs(15),
            workdir,
            token: None,
        }
        .run(&req)?
    };
    let md = to_markdown(&outcome, &scan);
    std::fs::write(outcome.workdir.join("ummp-report.md"), &md)?;
    if let Some(p) = report {
        std::fs::write(p, &md)?;
    }
    println!("\n{md}");
    let (passed, run) = score(&outcome);
    Ok(passed == run)
}

fn main() {
    tracing_subscriber::fmt::init();
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run_pack(cmd: PackCmd) -> Result<i32, Box<dyn std::error::Error>> {
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
            let kb = if kb.is_none() && mappings.is_none() {
                KnowledgeBase::open(Path::new(":memory:"))?
            } else {
                open_kb(kb, mappings)?
            };
            let formats = format
                .iter()
                .map(|f| pack_format(f))
                .collect::<Result<Vec<_>, _>>()?;
            run_pack_migrate(
                &MigrateOptions {
                    input: pack,
                    to,
                    loader,
                    from_loader,
                    from_mc,
                    online,
                    sources,
                    side: side(&s)?,
                    output,
                    allow_risk,
                    loader_version,
                    formats,
                    threads: 4,
                    min_confidence,
                },
                &kb,
            )
        }
        PackCmd::Export {
            pack,
            to_format,
            output,
        } => {
            let work = tempfile::tempdir()?;
            let p = import_with(&pack, work.path(), &ImportOptions::default())?;
            let (Some(mc), Some(loader)) = (p.mc.clone(), p.loader) else {
                return Err(
                    "cannot tell the pack's Minecraft version or loader; use `pack migrate`".into(),
                );
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
                    output,
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

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let pm = DefaultProjectManager;

    match cli.command {
        Commands::Analyze { path, json } => {
            let project = pm.open(&path)?;
            let fs: Arc<dyn FileSystem> = Arc::new(LocalFileSystem);
            let report = AnalyzerEngine::default().analyze(&project.root, &fs)?;
            if json {
                println!("{}", report.to_json()?);
            } else {
                print!("{}", report.to_text());
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
            let project = pm.open(&path)?;
            let kb = open_kb(kb, mappings)?;
            let request = MigrationRequest {
                source_minecraft: from,
                target_minecraft: to,
                target_loader: loader.to_lowercase(),
                min_confidence,
                source_loader: from_loader
                    .map(|l| l.to_lowercase())
                    .or_else(|| detected_loader(&project.root)),
            };
            let plan = MigrationPlanner::plan(&kb, &request)?;
            let snap_dir = snapshots.unwrap_or_else(|| default_snapshots(&project.root));
            let mut store = load_store(&snap_dir)?;

            let result = MigrationEngine::default().run(
                &plan,
                &project,
                &mut store,
                &RunOptions {
                    dry_run: !apply,
                    snapshot_root: &snap_dir,
                    session_id: Uuid::new_v4(),
                },
            )?;
            if result.checkpoint_id.is_some() {
                save_store(&snap_dir, &store)?;
            }

            let md = result.to_markdown();
            print!("{md}");
            if let Some(out) = report {
                let is_json = out.extension().is_some_and(|e| e == "json");
                let body = if is_json { result.to_json()? } else { md };
                std::fs::write(&out, body)?;
            }
            if !apply && !result.changes.is_empty() {
                eprintln!("\nDry run: nothing was written. Re-run with --apply to write.");
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
            let kb = open_kb(kb, mappings)?;
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
                return Err("no qualified class mappings for this request".into());
            }
            let stats = remap_jar(&input, &output, &stages)?;
            println!(
                "Wrote {}: {} classes remapped, {} class files moved, {} signature files dropped",
                output.display(),
                stats.classes_changed,
                stats.entries_moved,
                stats.signatures_dropped
            );
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
            let kb = open_kb(kb, mappings)?;
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
            let provider = provider(&ai)?;
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
            let provider = provider(&ai)?;
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
            print_ai_report(&report, &cfg);
        }

        Commands::Scan { path, json } => {
            let report = scan_path(&path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.to_text());
            }
            if report.risk() == Some(Severity::High) {
                std::process::exit(2);
            }
        }

        Commands::Build { path, opts } => {
            if !run_build(&path, opts)? {
                std::process::exit(1);
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
            let kb = if kb.is_none() && mappings.is_none() {
                println!("No knowledge base given: structural migration only (descriptors, build files, loader conversion).");
                KnowledgeBase::open(Path::new(":memory:"))?
            } else {
                open_kb(kb, mappings)?
            };
            let result = port(
                &PortOptions {
                    input,
                    output,
                    to,
                    loader,
                    from,
                    zip,
                    min_confidence,
                },
                &kb,
            )?;
            let r = &result.report;
            let count = |h: Handling| r.technologies.iter().filter(|f| f.handling == h).count();
            println!("Ported source: {}", result.output.display());
            if let Some(z) = &result.zip {
                println!("Zip: {}", z.display());
            }
            println!(
                "Changed or new files: {}, deleted: {}; findings: {} auto, {} assisted, {} manual",
                r.changes.len(),
                r.deletes.len(),
                count(Handling::Auto),
                count(Handling::Assisted),
                count(Handling::Manual)
            );
            println!("Report: {}", result.output.join("UMMP_REPORT.md").display());
            if build && !run_build(&result.output, build_opts)? {
                std::process::exit(1);
            }
        }

        Commands::Pack { cmd } => {
            let code = run_pack(cmd)?;
            if code != 0 {
                std::process::exit(code);
            }
        }

        Commands::Adapters => {
            println!("{:<20} level", "adapter");
            for (id, level) in AdapterRegistry::default().list() {
                println!("{id:<20} {level:?}");
            }
        }

        Commands::Rollback {
            path,
            snapshots,
            checkpoint,
        } => {
            let project = pm.open(&path)?;
            let snap_dir = snapshots.unwrap_or_else(|| default_snapshots(&project.root));
            let store = load_store(&snap_dir)?;
            let id = match checkpoint {
                Some(id) => id,
                None => {
                    store
                        .latest()
                        .ok_or("no checkpoints found in the snapshot directory")?
                        .id
                }
            };
            store.restore(id, &project.root)?;
            println!("Restored {} to checkpoint {id}", project.root.display());
        }
    }

    Ok(())
}
