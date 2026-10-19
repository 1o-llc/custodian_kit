//! `custodian bundle <job>`: one annotated file an agent reads whole (the
//! `bundles` format: `# start of path`, the content, `# end of path`),
//! holding a job's own files, its receipts, and the bounded source windows
//! around each site its readings name (five lines either side). A window of
//! an item at `secret` or above (a row the reading marked `held`) is held
//! through `classified` while it is written and goes to bundles as a
//! redaction, the one marker line, unless the bundle is asked to reveal it.

use crate::reading::refusal::ReadingRefusal;
use crate::reading::{load, ReadingReport};
use crate::store::write_whole;
use crate::Result;
use bundles::redaction::{LineRedaction, Redact};
use bundles::{BundleWriter, FileEntry};
use classified::ClassifiedString;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Lines either side of a site.
pub const WINDOW: u32 = 5;

/// The largest window held in one classified container.
const HOLD_LIMIT: usize = 1 << 20;

fn window(root: &Path, file: &str, line: u32) -> Option<(u32, u32, String)> {
    let text = std::fs::read_to_string(root.join(file)).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let start = line.saturating_sub(WINDOW).max(1);
    let end = (line + WINDOW).min(lines.len() as u32);
    let body = lines.get(start as usize - 1..end as usize)?.join("\n");
    Some((start, end, body))
}

/// One entry of a job bundle: its path in the bundle, its bytes, and the
/// level its body is held at (its first line, the window's heading, is not).
pub struct Entry {
    pub path: String,
    pub text: String,
    pub held: Option<String>,
}

/// A job folder's own files (job.toml, specs, steps, readings, the closing
/// receipt), whole, under `job/`.
fn folder_entries(folder: &Path) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if name != "bundle" {
                    stack.push(p);
                }
                continue;
            }
            if name.ends_with(".bundle") || name.starts_with('.') {
                continue;
            }
            let rel = p.strip_prefix(folder).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            let bytes = std::fs::read(&p).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", p.display())))?;
            out.push(Entry { path: format!("job/{rel}"), text: String::from_utf8_lossy(&bytes).into_owned(), held: None });
        }
    }
    Ok(out)
}

/// A job folder, its receipts and the windows around its sites, as one
/// bundle file `<folder>/<title>.bundle`. Returns the file, its digest and
/// how many windows were held redacted.
pub fn bundle_job(root: &Path, folder: &Path, title: &str, receipts: &[(i64, String)], reveal: bool) -> Result<(PathBuf, String, usize)> {
    let mut entries = folder_entries(folder)?;
    for (seq, json) in receipts {
        entries.push(Entry { path: format!("receipts/{seq}.json"), text: json.clone(), held: None });
    }
    let readings: Vec<ReadingReport> = job_readings(folder).into_iter().map(|(_, r)| r).collect();
    let wins = windows(root, &readings);
    entries.extend(wins);
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let (bytes, held) = write_entries(&entries, reveal)?;
    let out = folder.join(format!("{title}.bundle"));
    write_whole(&out, &bytes)?;
    Ok((out, crate::digest::tagged(&bytes), held))
}

/// The entries as one bundle: a held body goes in a classified container and
/// to bundles as a redaction of its lines, unless `reveal`. Returns the bytes
/// and how many were held.
pub fn write_entries(entries: &[Entry], reveal: bool) -> Result<(Vec<u8>, usize)> {
    let refuse = |e: bundles::BundleError| ReadingRefusal::Entry.refuse(e);
    let mut w = BundleWriter::new(Vec::new(), crate::bundle::PREFIX).map_err(refuse)?;
    for e in entries {
        match (&e.held, reveal) {
            (Some(level), false) => {
                let n = e.text.lines().count().max(1);
                let redact = if n > 1 { Redact::Lines(vec![LineRedaction { start: 2, end: n, level: level.clone() }]) } else { Redact::Whole { level: level.clone() } };
                let kept = ClassifiedString::try_new(e.text.clone(), HOLD_LIMIT).map_err(|x| ReadingRefusal::Hold.refuse(format!("{}: {x:?}", e.path)))?;
                kept.expose(|t| w.entry(FileEntry::bytes(e.path.as_str(), t.as_bytes()).with_redact(redact))).map_err(refuse)?;
            }
            _ => w.entry(FileEntry::bytes(e.path.as_str(), e.text.as_bytes())).map_err(refuse)?,
        }
    }
    let (bytes, report) = w.finish().map_err(refuse)?;
    Ok((bytes, report.redacted.len()))
}

/// The bounded windows around every site the readings name, under
/// `windows/`, a held row's window marked held at its level.
pub fn windows(root: &Path, readings: &[ReadingReport]) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for r in readings {
        for row in r.query.results.iter().chain(&r.unresolved) {
            let (Some(file), Some(line)) = (&row.path, row.line) else { continue };
            if !seen.insert((file.clone(), line)) || !file.ends_with(".rs") {
                continue;
            }
            let Some((start, end, body)) = window(root, file, line) else { continue };
            let held = row.attributes.get("held").filter(|h| h.as_str() == "true").map(|_| row.attributes.get("sensitivity").cloned().unwrap_or_else(|| crate::bundle::HELD_FROM.to_string()));
            let name = format!("windows/{}.L{start}-{end}.txt", file.replace('/', "__"));
            out.push(Entry { path: name, text: format!("// {file}:{start}-{end} ({} {})\n{body}\n", row.kind, row.label), held });
        }
    }
    out
}

/// A bundled job unbundled into `queued/<name>`: its job folder's files,
/// ready to start under a new id. The bundle's own receipts and windows stay
/// beside it under `imported/`.
pub fn unbundle_job(bundle: &Path, queued: &Path) -> Result<(PathBuf, String)> {
    let digest = crate::digest::tagged(std::fs::read(bundle).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", bundle.display())))?);
    let tmp = queued.with_extension("unbundle");
    bundles::unbundle_files(bundle, &tmp, "#").map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", bundle.display())))?;
    let job = tmp.join("job");
    if !job.is_dir() {
        return Err(ReadingRefusal::MissingInput.refuse(format!("{}: not a job bundle (no job/)", bundle.display())));
    }
    std::fs::rename(&job, queued).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", queued.display())))?;
    std::fs::rename(&tmp, queued.join("imported")).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", queued.display())))?;
    Ok((queued.to_path_buf(), digest))
}

/// The readings a job folder holds.
pub fn job_readings(folder: &Path) -> Vec<(PathBuf, ReadingReport)> {
    let mut out = Vec::new();
    let mut stack = vec![folder.join(crate::jobs::READINGS_DIR)];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "json") && p.file_name().is_some_and(|n| n != "latest.json") {
                if let Ok(r) = load(&p) {
                    out.push((p.with_extension("md"), r));
                }
            }
        }
    }
    out
}
