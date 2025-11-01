use anyhow::{Context, Result};
use colored::Colorize;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

fn is_allowed_path(path: &Path) -> bool {
    if let Some(s) = path.to_str() {
        return s.contains("src/utils/logging") || s.ends_with("src/utils/logging.rs");
    }
    false
}

#[derive(Debug)]
struct Violation {
    file: PathBuf,
    line_no: usize,
    col_start: usize,
    col_end: usize,
    line_text: String,
}

fn highlight_match(line: &str, col_start: usize, col_end: usize) -> String {
    let before = &line[..col_start];
    let matched = &line[col_start..col_end];
    let after = &line[col_end..];
    format!("{}{}{}", before, matched.red().bold(), after)
}

fn calc_col_in_line(_line: &str, byte_index_in_file: usize, line_start_in_file: usize) -> usize {
    byte_index_in_file.saturating_sub(line_start_in_file)
}

fn main() -> Result<()> {
    let start = Instant::now();
    let repo_root = std::env::current_dir()?;
    let re = Regex::new(r"\blog::(info|warn|debug|trace)\b")?;

    let mut violations: Vec<Violation> = Vec::new();
    let mut files_scanned: usize = 0usize;

    for entry in WalkDir::new(&repo_root)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(name == "target" || name == ".git" || name == "node_modules")
        })
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().map(|ext| ext == "rs").unwrap_or(false)
        })
    {
        let path = entry.path().to_path_buf();
        files_scanned += 1;

        if is_allowed_path(&path) {
            continue;
        }

        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read file {}", path.display()))?;

        for mat in re.find_iter(&text) {
            let before = &text[..mat.start()];
            let line_no = before.matches('\n').count() + 1;

            let last_newline_pos = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
            let line_start_index = last_newline_pos;
            let line_end_index = text[line_start_index..]
                .find('\n')
                .map(|p| line_start_index + p)
                .unwrap_or(text.len());

            let line_text = text[line_start_index..line_end_index].to_string();
            let col_start = calc_col_in_line(&line_text, mat.start(), line_start_index);
            let col_end = calc_col_in_line(&line_text, mat.end(), line_start_index);

            violations.push(Violation {
                file: path.clone(),
                line_no,
                col_start,
                col_end,
                line_text,
            });
        }
    }

    let total_violations = violations.len();
    let mut per_file_count = std::collections::BTreeMap::<PathBuf, usize>::new();
    for v in &violations {
        *per_file_count.entry(v.file.clone()).or_default() += 1;
    }

    println!("{}", "==== Logging Usage Check ====".bold());
    println!(
        "Scanned {} rust files in {:.2?}",
        files_scanned,
        start.elapsed()
    );

    if total_violations == 0 {
        println!(
            "{}",
            "No forbidden log::{info|warn|debug|trace} usages found.".green()
        );
        return Ok(());
    }

    println!(
        "{} {}",
        "Found".red().bold(),
        format!("{} forbidden logging usage(s)", total_violations)
            .red()
            .bold()
    );
    println!();

    println!("{}", "Summary by file:".bold().underline());
    for (file, count) in &per_file_count {
        println!(
            "  {}  {}",
            format!("{:>3}x", count).yellow(),
            file.display()
        );
    }
    println!();

    println!("{}", "Details:".bold().underline());
    for (file, vcount) in &per_file_count {
        println!("{} {}", "File:".cyan().bold(), file.display());
        println!("  {} violations", vcount);
        for v in violations.iter().filter(|vv| &vv.file == file) {
            let highlighted = highlight_match(&v.line_text, v.col_start, v.col_end);
            println!(
                "    {}:{}: {}",
                file.display(),
                v.line_no.to_string().yellow(),
                highlighted
            );
            if v.line_text.len() > 200 {
                println!("      {}", "...(line truncated)".dimmed());
            }
        }
        println!();
    }

    println!("{}", "Guidance:".bold().underline());
    println!(
        "  - Allowed location: {}",
        "src/utils/logging".green().bold()
    );
    println!("  - Suggested fixes:");
    println!("    * Move logging calls to the allowed module.");
    println!(
        "    * Use other facilities (e.g. return values, events) instead of direct log calls where appropriate."
    );
    println!();

    eprintln!(
        "{} {} violations in {} files. See details above.",
        "ERROR:".red().bold(),
        total_violations,
        per_file_count.len()
    );
    std::process::exit(1);
}
