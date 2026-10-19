//! `custodian bundle <job|reading>`: one annotated file an agent reads whole
//! (the `bundles` format: `# start of path`, the content, `# end of path`),
//! holding a job's closing receipt, its readings, and the bounded source
//! windows around each site the readings name (five lines either side). A
//! window in an item at `secret` or above is held through `classified` and
//! written redacted unless the bundle is asked to reveal it.

use crate::reading::refusal::ReadingRefusal;
use crate::reading::{load, ReadingReport};
use crate::store::write_whole;
use crate::Result;
use classified::ClassifiedString;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Lines either side of a site.
pub const WINDOW: u32 = 5;

/// The levels whose windows are held redacted.
pub const HELD: &[&str] = &["secret", "key_material"];

fn window(root: &Path, file: &str, line: u32) -> Option<(u32, u32, String)> {
    let text = std::fs::read_to_string(root.join(file)).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let start = line.saturating_sub(WINDOW).max(1);
    let end = (line + WINDOW).min(lines.len() as u32);
    let body = lines.get(start as usize - 1..end as usize)?.join("\n");
    Some((start, end, body))
}

/// Copies a job folder's own files (job.toml, specs, steps, readings, the
/// closing receipt) into the stage, whole.
fn stage_folder(folder: &Path, stage: &Path) -> Result<usize> {
    let mut n = 0;
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
            let rel = p.strip_prefix(folder).unwrap_or(&p);
            let bytes = std::fs::read(&p).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{}: {e}", p.display())))?;
            write_whole(&stage.join("job").join(rel), &bytes)?;
            n += 1;
        }
    }
    Ok(n)
}

/// A job folder, its receipts and the windows around its sites, as one
/// bundle file `<folder>/<title>.bundle`. Returns the file, its digest and
/// how many windows were held redacted.
pub fn bundle_job(root: &Path, folder: &Path, title: &str, receipts: &[(i64, String)], reveal: bool) -> Result<(PathBuf, String, usize)> {
    let stage = folder.join("bundle");
    stage_folder(folder, &stage)?;
    for (seq, json) in receipts {
        write_whole(&stage.join("receipts").join(format!("{seq}.json")), json.as_bytes())?;
    }
    let readings: Vec<ReadingReport> = job_readings(folder).into_iter().map(|(_, r)| r).collect();
    let held = windows(root, &readings, &stage.join("windows"), reveal)?;
    let out = folder.join(format!("{title}.bundle"));
    bundles::bundler::bundle_files(&stage, &out, &bundles::bundler::BundleOptions::default()).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", out.display())))?;
    let digest = crate::digest::tagged(std::fs::read(&out).unwrap_or_default());
    Ok((out, digest, held))
}

/// The bounded windows around every site the readings name, a secret item's
/// held through classified and written redacted unless `reveal`.
pub fn windows(root: &Path, readings: &[ReadingReport], dir: &Path, reveal: bool) -> Result<usize> {
    let mut held = 0;
    let mut seen = BTreeSet::new();
    for r in readings {
        for row in r.query.results.iter().chain(&r.unresolved) {
            let (Some(file), Some(line)) = (&row.path, row.line) else { continue };
            if !seen.insert((file.clone(), line)) || !file.ends_with(".rs") {
                continue;
            }
            let Some((start, end, body)) = window(root, file, line) else { continue };
            let secret = row.attributes.get("sensitivity").is_some_and(|l| HELD.contains(&l.as_str()));
            let text = if secret {
                held += 1;
                let kept = ClassifiedString::try_new(body, 1 << 20).map_err(|e| ReadingRefusal::Write.refuse(format!("{e:?}")))?;
                if reveal {
                    kept.expose(|s| s.to_string())
                } else {
                    format!("[REDACTED: {} lines of an item at {}; bundle --reveal shows them]", end - start + 1, row.attributes.get("sensitivity").cloned().unwrap_or_default())
                }
            } else {
                body
            };
            let name = format!("{}.L{start}-{end}.txt", file.replace('/', "__"));
            write_whole(&dir.join(name), format!("// {file}:{start}-{end} ({} {})\n{text}\n", row.kind, row.label).as_bytes())?;
        }
    }
    Ok(held)
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
    let mut stack = vec![folder.join("readings")];
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

/// A `bundle` reading's files as one bundle file beside its page: each file
/// staged whole, the held ranges replaced by a redaction line (unless the
/// line was run with `--reveal`), then bundled.
pub fn bundle_reading(root: &Path, report: &ReadingReport, out: &Path) -> Result<PathBuf> {
    let stage = out.with_extension("stage");
    let reveal = std::env::var_os(crate::reading::kinds::REVEAL_ENV).is_some();
    for row in report.query.results.iter().filter(|r| r.kind == "bundled-file") {
        let Some(rel) = &row.path else { continue };
        let text = std::fs::read_to_string(root.join(rel)).map_err(|e| ReadingRefusal::MissingInput.refuse(format!("{rel}: {e}")))?;
        let held: Vec<(u32, u32)> = row
            .attributes
            .get("held")
            .map(|h| h.split(',').filter_map(|r| r.split_once('-')).filter_map(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))).collect())
            .unwrap_or_default();
        let mut out_text = String::new();
        let mut skipping: Option<(u32, u32)> = None;
        for (i, line) in text.lines().enumerate() {
            let n = i as u32 + 1;
            if let Some((a, b)) = held.iter().copied().find(|(a, b)| n >= *a && n <= *b).filter(|_| !reveal) {
                if skipping != Some((a, b)) {
                    let kept = ClassifiedString::try_new(text.lines().skip(a as usize - 1).take((b - a + 1) as usize).collect::<Vec<_>>().join("\n"), 1 << 22).map_err(|e| ReadingRefusal::Write.refuse(format!("{e:?}")))?;
                    out_text.push_str(&format!("// [REDACTED: lines {a}-{b}, an item at secret or above; {} bytes held, not written]\n", kept.len()));
                    skipping = Some((a, b));
                }
                continue;
            }
            out_text.push_str(line);
            out_text.push('\n');
        }
        write_whole(&stage.join(rel), out_text.as_bytes())?;
    }
    bundles::bundler::bundle_files(&stage, out, &bundles::bundler::BundleOptions { comment_prefix: "#".into(), ..Default::default() }).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", out.display())))?;
    Ok(out.to_path_buf())
}
