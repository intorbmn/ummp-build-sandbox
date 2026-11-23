# Phase 10 plan (Native execution)

One task per guide milestone; each is test-first (red, then green) and ends with the four commands.
Skills used: brainstorming, writing-plans, executing-plans, test-driven-development, karpathy-guidelines,
rust-errors/idiomatic/testing/concurrency/performance/filesystem, architecture-decision-records; finish with
verification-before-completion and rust-review.

| Task | Files | Tests (core/tests/modpack.rs unless noted) |
|---|---|---|
| 10.A import | `modpack/{mod,formats}.rs`, `Cargo.toml` (sha1, sha2) | imports_mrpack_files_and_overrides★, rejects_mrpack_with_path_traversal★, rejects_zip_slip_in_overrides, rejects_mrpack_hash_mismatch, caps_archive_entry_size, imports_curseforge_manifest_as_unresolved, imports_prism_instance_components, imports_plain_folder_majority_loader |
| 10.B graph | `modpack/{analyze,graph}.rs`, `analyzer::inspect_jar` made `pub` | graph_reports_missing_required_dependency★, ..duplicate_mod_id, ..cycle_without_hanging, ..side_mismatch, pack_with_high_risk_jar_is_refused★, parallel_analysis_matches_sequential, analysis_is_deterministic |
| 10.C plan | `modpack/plan.rs` | plan_keeps_mod_that_supports_target★, plan_updates_when_index_has_target_version★, plan_ports_when_source_supplied, plan_replaces_by_rule, plan_marks_manual_with_reason, plan_rechecks_dependencies_after_decisions, offline_mode_never_calls_index |
| 10.D client | `providers/src/modrinth.rs` | by_hash_parses_version_file, versions_request_has_loader_and_game_version_params, sends_user_agent, http_429_retried_once_then_error, invalid_json_is_error_not_panic |
| 10.E config | `modpack/config.rs`, `pack_config_rules.toml` | config_rule_renames_key★, config_without_rule_is_copied_unchanged, config_rollback_restores_original |
| 10.F export/CLI | `modpack/{formats,report}.rs`, `cli/src/main.rs`, `cli/tests/pack_cli.rs` | export_round_trips★, exported_index_has_required_fields, report_lists_every_mod_once_with_action_and_reason, CLI inspect/migrate + exit codes |
| 10.G | docs, phase file, ADR, BACKLOG | synthetic pack run, 300-JAR timing, mutation checks, CI, PR |
