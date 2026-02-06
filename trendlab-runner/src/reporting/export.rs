//! Export orchestrator for artifacts and reports.

use anyhow::Result;
use std::path::Path;

use crate::reporting::artifacts::{ArtifactManager, ArtifactPaths};
use crate::reporting::reports::MarkdownReportGenerator;
use crate::result::BacktestResult;

pub fn export_run_with_report(
    output_dir: impl AsRef<Path>,
    result: &BacktestResult,
    include_report: bool,
) -> Result<ArtifactPaths> {
    let manager = ArtifactManager::new(output_dir)?;
    let mut paths = manager.save_run(result)?;

    if include_report {
        let report_path = paths
            .manifest
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join("report.md");
        let generator = MarkdownReportGenerator;
        let report = generator.generate(result);
        std::fs::write(&report_path, report)?;
        paths.report_markdown = Some(report_path);
    }

    Ok(paths)
}
