use serde::Serialize;

use crate::error::PgsError;

use crate::models::{LineOrigin, OperationPreview, PreviewLine};

use super::view::{
    CliErrorOutput, CommandOutput, CommitOutput, FileStatusView, LineOriginView, OperationOutput,
    OperationStatusView, OutputCommand, OverviewOutput, ScanDetail, ScanFileView, ScanHunkView,
    ScanLineView, ScanOutput, StatusOutput,
};

const MARKER_PREFIX: &str = "@@pgs:v1";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
struct StatusBoundaryRecord {
    command: OutputCommand,
    items: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
struct OverviewBoundaryRecord {
    command: OutputCommand,
    unstaged_files: usize,
    staged_files: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
struct OperationBoundaryRecord<'a> {
    command: OutputCommand,
    status: OperationStatusView,
    items: usize,
    backup_id: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct WarningRecord<'a> {
    message: &'a str,
}

#[derive(Debug, Serialize)]
struct PreviewBoundaryRecord<'a> {
    command: OutputCommand,
    selection: &'a str,
    file_path: &'a str,
    lines: usize,
    truncated: bool,
    limit_applied: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct PreviewLineRecord<'a> {
    file_path: &'a str,
    line_number: u32,
    origin: &'static str,
    content: &'a str,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
struct StatusSummaryRecord {
    command: OutputCommand,
    total_files: usize,
    total_additions: u32,
    total_deletions: u32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
struct ScanBoundaryRecord {
    command: OutputCommand,
    detail: ScanDetail,
    items: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
struct ScanSummaryRecord {
    command: OutputCommand,
    detail: ScanDetail,
    total_files: usize,
    total_hunks: usize,
    added: usize,
    modified: usize,
    deleted: usize,
    renamed: usize,
    binary: usize,
    mode_changed: usize,
}

#[derive(Debug, Serialize)]
struct ScanFileRecord<'a> {
    path: &'a str,
    status: &'a FileStatusView,
    binary: bool,
    hunks_count: usize,
    lines_added: u32,
    lines_deleted: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct ScanHunkRecord<'a> {
    path: &'a str,
    id: &'a str,
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    header: &'a str,
    additions: u32,
    deletions: u32,
    whitespace_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<&'a str>,
}

pub fn render(output: &CommandOutput) -> Result<String, PgsError> {
    match output {
        CommandOutput::Scan(scan) => render_scan(scan),
        CommandOutput::Operation(operation) => render_operation(operation),
        CommandOutput::Status(status) => render_status(status),
        CommandOutput::Commit(commit) => render_commit(commit),
        CommandOutput::Log(log) => render_marker("log", log),
        CommandOutput::Overview(overview) => render_overview(overview),
    }
}

pub fn render_error(output: &CliErrorOutput) -> Result<String, PgsError> {
    render_marker("error", output)
}

fn render_commit(output: &CommitOutput) -> Result<String, PgsError> {
    render_marker("commit.result", output)
}

fn render_operation(output: &OperationOutput) -> Result<String, PgsError> {
    let mut lines = Vec::with_capacity(output.items.len() + output.warnings.len() + 2);
    let boundary = OperationBoundaryRecord {
        command: output.command,
        status: output.status,
        items: output.items.len(),
        backup_id: output.backup_id.as_deref(),
    };

    lines.push(render_marker(
        &operation_marker_kind(output.command, "begin"),
        &boundary,
    )?);

    for item in &output.items {
        lines.push(render_marker("item", item)?);
    }

    for warning in &output.warnings {
        lines.push(render_marker(
            "warning",
            &WarningRecord { message: warning },
        )?);
    }

    if let Some(previews) = &output.previews {
        for preview in previews {
            render_preview(&mut lines, output.command, preview)?;
        }
    }

    lines.push(render_marker(
        &operation_marker_kind(output.command, "end"),
        &boundary,
    )?);

    Ok(lines.join("\n"))
}

fn render_preview(
    lines: &mut Vec<String>,
    command: OutputCommand,
    preview: &OperationPreview,
) -> Result<(), PgsError> {
    let boundary = PreviewBoundaryRecord {
        command,
        selection: &preview.selection,
        file_path: &preview.file_path,
        lines: preview.preview_lines.len(),
        truncated: preview.truncated,
        limit_applied: preview.limit_applied,
        reason: preview.reason.as_deref(),
    };
    let begin_kind = format!("{}.preview.begin", command.as_str());
    let line_kind = format!("{}.preview.line", command.as_str());
    let end_kind = format!("{}.preview.end", command.as_str());

    lines.push(render_marker(&begin_kind, &boundary)?);
    for line in &preview.preview_lines {
        lines.push(render_marker(
            &line_kind,
            &PreviewLineRecord::from((preview, line)),
        )?);
    }
    lines.push(render_marker(&end_kind, &boundary)?);
    Ok(())
}

impl<'a> From<(&'a OperationPreview, &'a PreviewLine)> for PreviewLineRecord<'a> {
    fn from((preview, line): (&'a OperationPreview, &'a PreviewLine)) -> Self {
        Self {
            file_path: &preview.file_path,
            line_number: line.line_number,
            origin: match line.origin {
                LineOrigin::Context => "context",
                LineOrigin::Addition => "addition",
                LineOrigin::Deletion => "deletion",
            },
            content: &line.content,
        }
    }
}

fn render_scan(output: &ScanOutput) -> Result<String, PgsError> {
    let mut lines = Vec::new();
    let boundary = ScanBoundaryRecord {
        command: output.command,
        detail: output.detail,
        items: output.files.len(),
    };

    lines.push(render_marker("scan.begin", &boundary)?);

    match output.detail {
        ScanDetail::Compact => render_scan_compact(&mut lines, output)?,
        ScanDetail::Full => render_scan_full(&mut lines, output)?,
    }

    lines.push(render_marker(
        "summary",
        &ScanSummaryRecord {
            command: output.command,
            detail: output.detail,
            total_files: output.summary.total_files,
            total_hunks: output.summary.total_hunks,
            added: output.summary.added,
            modified: output.summary.modified,
            deleted: output.summary.deleted,
            renamed: output.summary.renamed,
            binary: output.summary.binary,
            mode_changed: output.summary.mode_changed,
        },
    )?);
    lines.push(render_marker("scan.end", &boundary)?);

    Ok(lines.join("\n"))
}

fn render_status(output: &StatusOutput) -> Result<String, PgsError> {
    let mut lines = Vec::with_capacity(output.files.len() + 3);
    let boundary = StatusBoundaryRecord {
        command: output.command,
        items: output.files.len(),
    };

    lines.push(render_marker("status.begin", &boundary)?);

    for file in &output.files {
        lines.push(render_marker("status.file", file)?);
    }

    lines.push(render_marker(
        "summary",
        &StatusSummaryRecord {
            command: output.command,
            total_files: output.summary.total_files,
            total_additions: output.summary.total_additions,
            total_deletions: output.summary.total_deletions,
        },
    )?);

    lines.push(render_marker("status.end", &boundary)?);

    Ok(lines.join("\n"))
}

fn render_scan_compact(lines: &mut Vec<String>, output: &ScanOutput) -> Result<(), PgsError> {
    for file in &output.files {
        lines.push(render_marker("file", &ScanFileRecord::from(file))?);

        for hunk in &file.hunks {
            lines.push(render_marker("hunk", &ScanHunkRecord::from((file, hunk)))?);
        }
    }

    Ok(())
}

fn render_scan_full(lines: &mut Vec<String>, output: &ScanOutput) -> Result<(), PgsError> {
    for file in &output.files {
        let file_record = ScanFileRecord::from(file);
        lines.push(render_marker("file.begin", &file_record)?);

        for hunk in &file.hunks {
            let hunk_record = ScanHunkRecord::from((file, hunk));
            lines.push(render_marker("hunk.begin", &hunk_record)?);

            if let Some(hunk_lines) = &hunk.lines {
                lines.extend(hunk_lines.iter().map(render_diff_line));
            }

            lines.push(render_marker("hunk.end", &hunk_record)?);
        }

        lines.push(render_marker("file.end", &file_record)?);
    }

    Ok(())
}

impl<'a> From<&'a ScanFileView> for ScanFileRecord<'a> {
    fn from(file: &'a ScanFileView) -> Self {
        Self {
            path: &file.path,
            status: &file.status,
            binary: file.binary,
            hunks_count: file.hunks_count,
            lines_added: file.lines_added,
            lines_deleted: file.lines_deleted,
            checksum: file.checksum.as_deref(),
        }
    }
}

impl<'a> From<(&'a ScanFileView, &'a ScanHunkView)> for ScanHunkRecord<'a> {
    fn from((file, hunk): (&'a ScanFileView, &'a ScanHunkView)) -> Self {
        Self {
            path: &file.path,
            id: &hunk.id,
            old_start: hunk.old_start,
            old_lines: hunk.old_lines,
            new_start: hunk.new_start,
            new_lines: hunk.new_lines,
            header: &hunk.header,
            additions: hunk.additions,
            deletions: hunk.deletions,
            whitespace_only: hunk.whitespace_only,
            checksum: hunk.checksum.as_deref(),
        }
    }
}

fn render_overview(output: &OverviewOutput) -> Result<String, PgsError> {
    let boundary = OverviewBoundaryRecord {
        command: output.command,
        unstaged_files: output.unstaged.files.len(),
        staged_files: output.staged.files.len(),
    };

    let mut sections = vec![render_marker("overview.begin", &boundary)?];
    sections.push(render_scan(&output.unstaged)?);
    sections.push(render_status(&output.staged)?);
    sections.push(render_marker("overview.end", &boundary)?);

    Ok(sections.join("\n"))
}

fn render_diff_line(line: &ScanLineView) -> String {
    let prefix = match line.origin {
        LineOriginView::Context => ' ',
        LineOriginView::Addition => '+',
        LineOriginView::Deletion => '-',
    };

    format!("{prefix}{}", line.content)
}

fn operation_marker_kind(command: OutputCommand, suffix: &str) -> String {
    format!("{}.{suffix}", command.as_str())
}

fn render_marker<T: Serialize>(kind: &str, payload: &T) -> Result<String, PgsError> {
    let json = serde_json::to_string(payload)?;
    Ok(format!("{MARKER_PREFIX} {kind} {json}"))
}
