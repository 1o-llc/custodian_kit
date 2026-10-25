//! An archive: workspace crates named by their declared routes (`crate:<name>`), the crates they depend on inside
//! the workspace (the closure, when asked), and declared attached files, as one ZIP whose bytes are the same for
//! the same inputs, whose manifest (`META/archive-manifest.tsv`) records why every crate is in it (explicit, or a
//! dependency and of which) and the SHA-256 of every file, and which reads back as written.
//!
//! The crates and their path dependencies are what `cargo metadata --no-deps` says, run as the declared line
//! ([`crate::exec::CargoMetadata`]); a route is resolved through the declared table ([`crate::paths::PathsTable`]),
//! so a job archives declared routes only; an attached file is a route too (`crate:<name>/<file>`, or a `path:` the
//! table declares inside a job). A file holding an item at `secret` or above is the bundle's rule
//! ([`crate::bundle`]): an archive copies files whole, so it is refused rather than written open, unless the line
//! says `--reveal`. The ZIP is written beside its place and renamed into it, so no half-written archive is ever
//! published, and an archive that exists is never written over. Every digest is [`crate::digest`]'s.
//!
//! [`exec_archive`] is the body of the declared line `custodian exec-archive` ([`crate::exec::ExecArchive`]); the
//! command line runs that line through its facade with a receipt and reads the summary it prints last.

use crate::api::{Resolving, Scope, Workspace};
use crate::exec::{self, Cargo};
use crate::reading::refusal::ReadingRefusal;
use crate::reading::requirements::{cargo, Ran};
use crate::Result;
use klepto::CargoWorkspace;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The manifest's place in the archive.
pub const MANIFEST_PATH: &str = "META/archive-manifest.tsv";
/// The manifest's first line: its schema.
pub const SCHEMA: &str = "custodian.archive.v1";
const ZIP_LOCAL_HEADER: u32 = 0x0403_4b50;
const ZIP_CENTRAL_HEADER: u32 = 0x0201_4b50;
const ZIP_END: u32 = 0x0605_4b50;

/// One workspace member: its name as its manifest spells it, its directory, the members it depends on by path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Package {
    pub name: String,
    pub dir: PathBuf,
    pub local_dependencies: Vec<String>,
}

/// The workspace's members, as cargo lists them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Members {
    pub root: PathBuf,
    pub packages: BTreeMap<String, Package>,
}

/// A member in the archive depends on a member that is not.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyGap {
    pub from: String,
    pub dependency: String,
}

/// Why a crate is in the archive.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrateInclusionKind {
    Explicit,
    Dependency,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CrateProvenance {
    pub name: String,
    pub kind: CrateInclusionKind,
    /// The members that pull a dependency in.
    pub required_by: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttachmentProvenance {
    /// Relative to the root, slashes.
    pub source: String,
    pub archive_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct PlanOptions {
    pub include_workspace_dependencies: bool,
    /// Relative to the root.
    pub attachments: Vec<PathBuf>,
}

/// What the archive holds, planned: its files in archive order, each with its bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchivePlan {
    pub explicit: Vec<String>,
    pub crates: Vec<CrateProvenance>,
    pub gaps: Vec<DependencyGap>,
    pub attachments: Vec<AttachmentProvenance>,
    pub files: Vec<ArchiveFile>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveFile {
    pub path: String,
    /// Where it came from, relative to the root (not written to the archive).
    pub source: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub files: usize,
    pub selected: Vec<String>,
}

#[derive(Clone, Debug)]
struct ZipEntry {
    path: String,
    crc32: u32,
    offset: u32,
}

fn refused(what: impl std::fmt::Display) -> crate::CustodianError {
    ReadingRefusal::ArchiveRefused.refuse(what)
}

impl Members {
    /// The members of the workspace at `root`, by the declared `cargo metadata --no-deps` line run in it; what the
    /// line did is pushed on `ran`.
    pub fn read(root: &Path, ran: &mut Vec<Ran>) -> Result<Members> {
        let manifest = root.join("Cargo.toml");
        let line = Cargo::metadata(manifest.as_os_str()).format_version("1").no_deps();
        let w = cargo::run("archive-metadata", exec::line("metadata", &line), root, root, ran).ok_or_else(|| ReadingRefusal::LineNotStarted.refuse(ran.last().map(|r| r.said.clone()).unwrap_or_default()))?;
        if w.code != Some(0) {
            return Err(ReadingRefusal::LineRefused.refuse(format!("{}: exit {:?}: {}", w.operation.display, w.code, ran.last().map(|r| r.said.clone()).unwrap_or_default())));
        }
        let cw = CargoWorkspace::from_json(&w.stdout).map_err(|e| ReadingRefusal::NoWorkspace.refuse(format!("cargo metadata's JSON did not read: {e}")))?;
        Members::from_cargo(root, &cw)
    }

    /// The members as cargo read them: a path dependency on another member is a local dependency.
    pub fn from_cargo(root: &Path, cw: &CargoWorkspace) -> Result<Members> {
        let root = fs::canonicalize(root).map_err(|e| refused(format!("cannot resolve workspace root {}: {e}", root.display())))?;
        let mut dir_to_name: BTreeMap<PathBuf, String> = BTreeMap::new();
        let members = cw.member_packages();
        for p in &members {
            let dir = p.manifest_path.parent().ok_or_else(|| refused(format!("manifest has no parent directory: {}", p.manifest_path.display())))?;
            let canonical = fs::canonicalize(dir).map_err(|e| refused(format!("cannot resolve package directory {}: {e}", dir.display())))?;
            if dir_to_name.insert(canonical, p.name.clone()).is_some() {
                return Err(refused(format!("duplicate package directory for {}", p.name)));
            }
        }
        let mut packages = BTreeMap::new();
        for p in &members {
            // the directory as the root is spelled (canonical), so every file's source is relative to the root
            let dir = fs::canonicalize(p.manifest_path.parent().unwrap_or(Path::new("."))).map_err(|e| refused(format!("cannot resolve package directory of {}: {e}", p.name)))?;
            let mut local = BTreeSet::new();
            for d in p.dependencies.iter().filter_map(|d| d.path.as_ref()) {
                let at = if d.is_absolute() { d.clone() } else { dir.join(d) };
                let canonical = fs::canonicalize(&at).map_err(|e| refused(format!("cannot resolve local dependency {} from {}: {e}", at.display(), p.name)))?;
                if let Some(name) = dir_to_name.get(&canonical) {
                    local.insert(name.clone());
                }
            }
            let package = Package { name: p.name.clone(), dir, local_dependencies: local.into_iter().collect() };
            if packages.insert(package.name.clone(), package).is_some() {
                return Err(refused(format!("duplicate package name in workspace: {}", p.name)));
            }
        }
        if packages.is_empty() {
            return Err(refused(format!("no workspace members below {}", root.display())));
        }
        Ok(Members { root, packages })
    }

    /// The member whose directory is `dir` (a `crate:` route's root), by its name as its manifest spells it.
    pub fn by_dir(&self, dir: &Path) -> Option<&Package> {
        let canonical = fs::canonicalize(dir).ok()?;
        self.packages.values().find(|p| fs::canonicalize(&p.dir).ok().as_ref() == Some(&canonical))
    }

    pub fn plan(&self, selected: &[String]) -> Result<ArchivePlan> {
        self.plan_with_options(selected, &PlanOptions::default())
    }

    /// The archive of `selected` (members by name): the closure when asked, the gaps otherwise, each crate's files
    /// under `crates/<name>/`, each attachment under `workspace/`, every archive path once.
    pub fn plan_with_options(&self, selected: &[String], options: &PlanOptions) -> Result<ArchivePlan> {
        if selected.is_empty() {
            return Err(refused("at least one crate must be selected"));
        }
        let explicit_set: BTreeSet<_> = selected.iter().cloned().collect();
        if explicit_set.len() != selected.len() {
            return Err(refused("selected crates contain duplicates"));
        }
        for name in &explicit_set {
            if !self.packages.contains_key(name) {
                return Err(refused(format!("unknown workspace crate: {name}")));
            }
        }

        let mut resolved = explicit_set.clone();
        if options.include_workspace_dependencies {
            loop {
                let current: Vec<_> = resolved.iter().cloned().collect();
                let mut changed = false;
                for name in current {
                    for dependency in &self.packages[&name].local_dependencies {
                        changed |= resolved.insert(dependency.clone());
                    }
                }
                if !changed {
                    break;
                }
            }
        }

        let mut gaps = Vec::new();
        for name in &resolved {
            for dependency in &self.packages[name].local_dependencies {
                if !resolved.contains(dependency) {
                    gaps.push(DependencyGap { from: name.clone(), dependency: dependency.clone() });
                }
            }
        }
        gaps.sort_by(|left, right| (&left.from, &left.dependency).cmp(&(&right.from, &right.dependency)));

        let mut crates = Vec::new();
        for name in &resolved {
            if explicit_set.contains(name) {
                crates.push(CrateProvenance { name: name.clone(), kind: CrateInclusionKind::Explicit, required_by: Vec::new() });
            } else {
                let required_by: Vec<String> = resolved.iter().filter(|parent| self.packages[*parent].local_dependencies.contains(name)).cloned().collect();
                crates.push(CrateProvenance { name: name.clone(), kind: CrateInclusionKind::Dependency, required_by });
            }
        }

        let mut files = Vec::new();
        for name in &resolved {
            collect_package_files(&self.root, &self.packages[name], &mut files)?;
        }

        let mut attachments = Vec::new();
        for relative in &options.attachments {
            attachments.push(collect_attachment(&self.root, relative, &mut files)?);
        }
        attachments.sort_by(|left, right| left.source.cmp(&right.source));

        files.sort_by(|left, right| left.path.cmp(&right.path));
        ensure_unique_archive_paths(&files)?;

        Ok(ArchivePlan { explicit: explicit_set.into_iter().collect(), crates, gaps, attachments, files })
    }
}

impl ArchivePlan {
    /// The archive written at `output`, whole: a `.zip.partial` beside it, renamed into place once every byte is
    /// written; an `output` that exists is refused. Returns the digest of the bytes written.
    pub fn build_zip(&self, output: &Path) -> Result<String> {
        if output.exists() {
            return Err(refused(format!("refusing to overwrite {}", output.display())));
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| refused(format!("cannot create output directory {}: {e}", parent.display())))?;
        }
        let mut entries = Vec::with_capacity(self.files.len() + 1);
        entries.push(ArchiveFile { path: MANIFEST_PATH.to_owned(), source: String::new(), bytes: self.manifest_bytes() });
        entries.extend(self.files.iter().cloned());
        entries.sort_by(|left, right| left.path.cmp(&right.path));

        let archive = encode_zip(&entries)?;
        let temp = output.with_extension("zip.partial");
        if temp.exists() {
            fs::remove_file(&temp).map_err(|e| refused(format!("cannot remove stale temporary file {}: {e}", temp.display())))?;
        }
        fs::write(&temp, &archive).map_err(|e| ReadingRefusal::Write.refuse(format!("{}: {e}", temp.display())))?;
        fs::rename(&temp, output).map_err(|e| ReadingRefusal::Write.refuse(format!("cannot publish {}: {e}", output.display())))?;
        Ok(crate::digest::tagged(&archive))
    }

    /// The manifest: the schema, each crate and why, each attachment, each gap, each file's digest and size.
    #[must_use]
    pub fn manifest_bytes(&self) -> Vec<u8> {
        let mut manifest = format!("schema\t{SCHEMA}\n");
        for item in &self.crates {
            match item.kind {
                CrateInclusionKind::Explicit => manifest.push_str(&format!("crate\texplicit\t{}\n", item.name)),
                CrateInclusionKind::Dependency => manifest.push_str(&format!("crate\tdependency\t{}\t{}\n", item.name, item.required_by.join(","))),
            }
        }
        for a in &self.attachments {
            manifest.push_str(&format!("attached\t{}\t{}\n", a.source, a.archive_path));
        }
        for gap in &self.gaps {
            manifest.push_str(&format!("missing\t{}\t{}\n", gap.from, gap.dependency));
        }
        for file in &self.files {
            manifest.push_str(&format!("file\t{}\t{}\t{}\n", crate::digest::tagged(&file.bytes), file.bytes.len(), file.path));
        }
        manifest.into_bytes()
    }

    /// The bytes of every file, summed.
    pub fn bytes(&self) -> usize {
        self.files.iter().map(|f| f.bytes.len()).sum()
    }
}

fn invalid(what: impl std::fmt::Display) -> crate::CustodianError {
    ReadingRefusal::ArchiveInvalid.refuse(what)
}

/// The archive at `path` read back: every entry's CRC, the manifest's schema, every file it names present with
/// the digest and size it says, and no file it does not name.
pub fn verify_zip(path: &Path) -> Result<VerifyReport> {
    let bytes = fs::read(path).map_err(|e| invalid(format!("cannot read archive {}: {e}", path.display())))?;
    let entries = decode_zip(&bytes)?;
    let manifest = entries.get(MANIFEST_PATH).ok_or_else(|| invalid(format!("archive does not contain {MANIFEST_PATH}")))?;
    let manifest = std::str::from_utf8(manifest).map_err(|e| invalid(format!("manifest is not UTF-8: {e}")))?;

    let mut expected = BTreeMap::new();
    let mut selected = BTreeSet::new();
    let mut saw_schema = false;
    for line in manifest.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        match fields.as_slice() {
            ["schema", s] if *s == SCHEMA => saw_schema = true,
            ["crate", "explicit", name] | ["crate", "dependency", name, _] => {
                selected.insert((*name).to_owned());
            }
            ["attached", _, _] | ["missing", _, _] => {}
            ["file", digest, size, file_path] => {
                let size = size.parse::<usize>().map_err(|e| invalid(format!("invalid manifest size for {file_path}: {e}")))?;
                if expected.insert((*file_path).to_owned(), ((*digest).to_owned(), size)).is_some() {
                    return Err(invalid(format!("duplicate manifest file entry: {file_path}")));
                }
            }
            _ => return Err(invalid(format!("invalid manifest line: {line}"))),
        }
    }
    if !saw_schema {
        return Err(invalid(format!("manifest does not contain the schema line `{SCHEMA}`")));
    }
    if selected.is_empty() {
        return Err(invalid("manifest contains no crates"));
    }
    for (file_path, (digest, size)) in &expected {
        let payload = entries.get(file_path).ok_or_else(|| invalid(format!("archive is missing {file_path}")))?;
        if payload.len() != *size {
            return Err(invalid(format!("size mismatch for {file_path}")));
        }
        if &crate::digest::tagged(payload) != digest {
            return Err(invalid(format!("digest mismatch for {file_path}")));
        }
    }
    let payload_count = entries.len().saturating_sub(1);
    if payload_count != expected.len() {
        return Err(invalid(format!("archive contains {payload_count} payload entries but manifest declares {}", expected.len())));
    }
    Ok(VerifyReport { files: expected.len(), selected: selected.into_iter().collect() })
}

fn collect_package_files(root: &Path, package: &Package, output: &mut Vec<ArchiveFile>) -> Result<()> {
    collect_files_recursive(root, &package.dir, &package.dir, &package.name, output)
}

fn collect_files_recursive(root: &Path, base: &Path, current: &Path, crate_name: &str, output: &mut Vec<ArchiveFile>) -> Result<()> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(current).map_err(|e| refused(format!("cannot read directory {}: {e}", current.display())))?.collect::<std::result::Result<_, _>>().map_err(|e| refused(format!("cannot read directory entry: {e}")))?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| refused(format!("cannot inspect {}: {e}", path.display())))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_symlink() {
            return Err(refused(format!("symlinks are not allowed in package: {}", path.display())));
        }
        if file_type.is_dir() {
            if matches!(name.as_ref(), ".git" | "target") {
                continue;
            }
            collect_files_recursive(root, base, &path, crate_name, output)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = path.strip_prefix(base).map_err(|e| refused(format!("cannot relativize {}: {e}", path.display())))?;
        validate_relative_path(relative)?;
        let archive_path = format!("crates/{crate_name}/{}", slash_path(relative)?);
        if archive_path == MANIFEST_PATH {
            return Err(refused("package collides with reserved archive manifest"));
        }
        let bytes = fs::read(&path).map_err(|e| refused(format!("cannot read {}: {e}", path.display())))?;
        output.push(ArchiveFile { path: archive_path, source: rel(root, &path), bytes });
    }
    Ok(())
}

fn collect_attachment(root: &Path, relative: &Path, output: &mut Vec<ArchiveFile>) -> Result<AttachmentProvenance> {
    validate_relative_path(relative)?;
    if relative.as_os_str().is_empty() {
        return Err(refused("attached file path cannot be empty"));
    }
    let source = root.join(relative);
    let metadata = fs::symlink_metadata(&source).map_err(|e| refused(format!("cannot inspect attached file {}: {e}", source.display())))?;
    if metadata.file_type().is_symlink() {
        return Err(refused(format!("attached file may not be a symlink: {}", source.display())));
    }
    if !metadata.is_file() {
        return Err(refused(format!("attached path is not a file: {}", source.display())));
    }
    let canonical = fs::canonicalize(&source).map_err(|e| refused(format!("cannot resolve attached file {}: {e}", source.display())))?;
    if !canonical.starts_with(root) {
        return Err(refused(format!("attached file escapes workspace root: {}", source.display())));
    }
    let source_relative = slash_path(relative)?;
    let archive_path = format!("workspace/{source_relative}");
    if archive_path == MANIFEST_PATH {
        return Err(refused("attached file collides with reserved archive manifest"));
    }
    let bytes = fs::read(&canonical).map_err(|e| refused(format!("cannot read attached file {}: {e}", source.display())))?;
    output.push(ArchiveFile { path: archive_path.clone(), source: source_relative.clone(), bytes });
    Ok(AttachmentProvenance { source: source_relative, archive_path })
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// A path inside the archive: relative, no `..`, no root.
fn validate_relative_path(path: &Path) -> Result<()> {
    if path.is_absolute() {
        return Err(refused(format!("absolute archive path rejected: {}", path.display())));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return Err(refused(format!("unsafe archive path rejected: {}", path.display()))),
        }
    }
    Ok(())
}

fn slash_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return Err(refused(format!("unsafe path: {}", path.display()))),
        }
    }
    Ok(parts.join("/"))
}

fn ensure_unique_archive_paths(files: &[ArchiveFile]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for file in files {
        if !seen.insert(&file.path) {
            return Err(refused(format!("duplicate archive path: {}", file.path)));
        }
    }
    Ok(())
}

/// ZIP32, every entry stored (no compression), UTF-8 names, no timestamps, no extra fields: the same bytes for the
/// same entries.
fn encode_zip(files: &[ArchiveFile]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut entries = Vec::with_capacity(files.len());
    for file in files {
        let path_bytes = file.path.as_bytes();
        let name_len = u16::try_from(path_bytes.len()).map_err(|_| refused(format!("archive path too long: {}", file.path)))?;
        let size = u32::try_from(file.bytes.len()).map_err(|_| refused(format!("file too large for ZIP32: {}", file.path)))?;
        let offset = u32::try_from(output.len()).map_err(|_| refused("archive exceeds ZIP32"))?;
        let crc32 = crc32(&file.bytes);
        push_u32(&mut output, ZIP_LOCAL_HEADER);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0x0800);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0x0021);
        push_u32(&mut output, crc32);
        push_u32(&mut output, size);
        push_u32(&mut output, size);
        push_u16(&mut output, name_len);
        push_u16(&mut output, 0);
        output.extend_from_slice(path_bytes);
        output.extend_from_slice(&file.bytes);
        entries.push(ZipEntry { path: file.path.clone(), crc32, offset });
    }
    let central_offset = u32::try_from(output.len()).map_err(|_| refused("archive exceeds ZIP32"))?;
    for entry in &entries {
        let path_bytes = entry.path.as_bytes();
        let name_len = u16::try_from(path_bytes.len()).map_err(|_| refused(format!("archive path too long: {}", entry.path)))?;
        let source = files.iter().find(|file| file.path == entry.path).ok_or_else(|| refused("internal archive entry mismatch"))?;
        let size = u32::try_from(source.bytes.len()).map_err(|_| refused(format!("file too large for ZIP32: {}", entry.path)))?;
        push_u32(&mut output, ZIP_CENTRAL_HEADER);
        push_u16(&mut output, 20);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0x0800);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0x0021);
        push_u32(&mut output, entry.crc32);
        push_u32(&mut output, size);
        push_u32(&mut output, size);
        push_u16(&mut output, name_len);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, entry.offset);
        output.extend_from_slice(path_bytes);
    }
    let central_size = u32::try_from(output.len()).map_err(|_| refused("archive exceeds ZIP32"))?.checked_sub(central_offset).ok_or_else(|| refused("invalid central directory size"))?;
    let count = u16::try_from(entries.len()).map_err(|_| refused("too many ZIP entries"))?;
    push_u32(&mut output, ZIP_END);
    push_u16(&mut output, 0);
    push_u16(&mut output, 0);
    push_u16(&mut output, count);
    push_u16(&mut output, count);
    push_u32(&mut output, central_size);
    push_u32(&mut output, central_offset);
    push_u16(&mut output, 0);
    Ok(output)
}

fn decode_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut cursor = 0usize;
    let mut entries = BTreeMap::new();
    while cursor + 4 <= bytes.len() {
        let signature = read_u32(bytes, cursor)?;
        if signature == ZIP_CENTRAL_HEADER || signature == ZIP_END {
            break;
        }
        if signature != ZIP_LOCAL_HEADER {
            return Err(invalid(format!("unexpected ZIP signature at byte {cursor}")));
        }
        if cursor + 30 > bytes.len() {
            return Err(invalid("truncated ZIP local header"));
        }
        let flags = read_u16(bytes, cursor + 6)?;
        let method = read_u16(bytes, cursor + 8)?;
        let expected_crc = read_u32(bytes, cursor + 14)?;
        let compressed_size = read_u32(bytes, cursor + 18)? as usize;
        let uncompressed_size = read_u32(bytes, cursor + 22)? as usize;
        let name_len = read_u16(bytes, cursor + 26)? as usize;
        let extra_len = read_u16(bytes, cursor + 28)? as usize;
        if flags != 0x0800 || method != 0 {
            return Err(invalid("the verifier accepts only deterministic stored UTF-8 entries"));
        }
        if compressed_size != uncompressed_size {
            return Err(invalid("stored ZIP entry has mismatched sizes"));
        }
        let name_start = cursor + 30;
        let name_end = name_start.checked_add(name_len).ok_or_else(|| invalid("ZIP path length overflow"))?;
        let data_start = name_end.checked_add(extra_len).ok_or_else(|| invalid("ZIP extra length overflow"))?;
        let data_end = data_start.checked_add(compressed_size).ok_or_else(|| invalid("ZIP payload length overflow"))?;
        if data_end > bytes.len() {
            return Err(invalid("truncated ZIP payload"));
        }
        let name = std::str::from_utf8(&bytes[name_start..name_end]).map_err(|e| invalid(format!("ZIP path is not UTF-8: {e}")))?.to_owned();
        validate_archive_name(&name)?;
        let payload = bytes[data_start..data_end].to_vec();
        if crc32(&payload) != expected_crc {
            return Err(invalid(format!("CRC mismatch for {name}")));
        }
        if entries.insert(name.clone(), payload).is_some() {
            return Err(invalid(format!("duplicate ZIP entry: {name}")));
        }
        cursor = data_end;
    }
    if entries.is_empty() {
        return Err(invalid("archive contains no local entries"));
    }
    Ok(entries)
}

fn validate_archive_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') || name.split('/').any(|part| matches!(part, "" | "." | "..")) {
        return Err(invalid(format!("unsafe archive entry name: {name}")));
    }
    Ok(())
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let slice = bytes.get(offset..offset + 2).ok_or_else(|| invalid("truncated ZIP integer"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let slice = bytes.get(offset..offset + 4).ok_or_else(|| invalid("truncated ZIP integer"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// CRC-32 (IEEE), as ZIP wants it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

// ---- the declared line's body ----

/// What `custodian exec-archive` is asked: the root, the routes (`crate:<name>`), the attached files as routes
/// (`crate:<name>/<file>`, `path:<rel>`), whether the closure is taken, whether held files are copied as they are,
/// where the archive goes (`None`: the plan only, nothing written), and who resolves the routes.
#[derive(Debug, Clone)]
pub struct ArchiveJob {
    pub root: PathBuf,
    pub routes: Vec<String>,
    pub attach: Vec<String>,
    pub workspace_deps: bool,
    pub reveal: bool,
    pub out: Option<PathBuf>,
    pub how: Resolving,
}

/// What the line printed last: the plan, what was written, what read back, and the cargo line it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveSummary {
    pub explicit: Vec<String>,
    pub crates: Vec<CrateProvenance>,
    pub gaps: Vec<DependencyGap>,
    pub attachments: Vec<AttachmentProvenance>,
    pub files: usize,
    pub bytes: usize,
    /// Files holding an item at `secret` or above, copied as they are (`--reveal`).
    pub revealed: Vec<String>,
    /// The archive written, and its digest.
    pub archive: Option<String>,
    pub digest: Option<String>,
    /// The archive read back after it was written.
    pub verified: Option<VerifyReport>,
    /// The cargo line, as the requirements reading records one.
    pub cargo: Vec<Ran>,
}

impl ArchiveSummary {
    /// The line's exit status: 1 when a member in the archive depends on a member that is not, else 0.
    pub fn code(&self) -> i32 {
        i32::from(!self.gaps.is_empty())
    }
}

/// The body of `custodian exec-archive --root R --include ROUTE... [--attach ROUTE]... [--workspace-deps]
/// [--reveal] [--out ZIP]`: the routes resolved through the declared table (a job: declared routes only), the
/// members read by the declared cargo line, the plan, the held-file rule, the archive written whole and read back.
pub fn exec_archive(job: &ArchiveJob) -> Result<ArchiveSummary> {
    let ws = Workspace::open_with(&job.root, &Scope::default(), job.how)?;
    let root = ws.root.clone();
    let mut ran = Vec::new();
    let members = Members::read(&root, &mut ran)?;
    let paths = ws.paths();
    let mut selected = Vec::new();
    for r in &job.routes {
        let route = paths.resolve(r, job.how)?;
        let key = route.crate_name().ok_or_else(|| refused(format!("`{r}` is not a crate: route (an archive holds whole crates; attach a file with --attach)")))?;
        let dir = paths.crates.get(key).ok_or_else(|| ReadingRefusal::UndeclaredRoute.refuse(r))?;
        let member = members.by_dir(dir).ok_or_else(|| refused(format!("`{r}` is not a member of the workspace cargo read at {}", root.display())))?;
        selected.push(member.name.clone());
    }
    let mut attachments = Vec::new();
    for a in &job.attach {
        let route = paths.resolve(a, job.how)?;
        let rel = match (route.crate_file(&root), route.walk_root(&root)) {
            (Some(f), _) => f,
            (None, Some(p)) if route.crate_name().is_none() => rel(&root, &p),
            _ => return Err(refused(format!("`{a}` is not a file route (crate:<name>/<file>, or path:<file>)"))),
        };
        attachments.push(PathBuf::from(rel));
    }
    let plan = members.plan_with_options(&selected, &PlanOptions { include_workspace_dependencies: job.workspace_deps, attachments })?;

    // the bundle's rule, one place: a file holding an item at secret or above is held; an archive copies files whole
    let held = crate::bundle::held_files(ws.facts())?;
    let in_plan: Vec<String> = plan.files.iter().filter(|f| held.contains(&f.source)).map(|f| f.source.clone()).collect();
    if !in_plan.is_empty() && !job.reveal {
        return Err(ReadingRefusal::ArchiveHeld.refuse(in_plan.join(", ")));
    }

    let mut summary = ArchiveSummary { explicit: plan.explicit.clone(), crates: plan.crates.clone(), gaps: plan.gaps.clone(), attachments: plan.attachments.clone(), files: plan.files.len(), bytes: plan.bytes(), revealed: in_plan, archive: None, digest: None, verified: None, cargo: ran };
    if let Some(out) = &job.out {
        let digest = plan.build_zip(out)?;
        let verified = verify_zip(out)?;
        summary.archive = Some(out.display().to_string());
        summary.digest = Some(digest);
        summary.verified = Some(verified);
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn rejects_unsafe_archive_names() {
        assert!(validate_archive_name("../escape").is_err());
        assert!(validate_archive_name("/absolute").is_err());
        assert!(validate_archive_name("a//b").is_err());
        assert!(validate_archive_name("a\\b").is_err());
        assert!(validate_relative_path(Path::new("../outside.txt")).is_err());
        assert!(validate_relative_path(Path::new("/etc/passwd")).is_err());
        assert!(validate_relative_path(Path::new("./docs/x.txt")).is_ok());
    }

    #[test]
    fn deterministic_zip_round_trip() {
        let files = vec![ArchiveFile { path: "a.txt".to_owned(), source: "a.txt".into(), bytes: b"alpha".to_vec() }, ArchiveFile { path: "b.txt".to_owned(), source: "b.txt".into(), bytes: b"beta".to_vec() }];
        let first = encode_zip(&files).expect("encode first archive");
        let second = encode_zip(&files).expect("encode second archive");
        assert_eq!(first, second);
        let decoded = decode_zip(&first).expect("decode archive");
        assert_eq!(decoded["a.txt"], b"alpha");
        assert_eq!(decoded["b.txt"], b"beta");
    }

    /// The three-crate fixture the prototype was tested on: `fixture_app -> fixture_mid -> fixture_leaf`, a README
    /// per crate, the workspace's own files to attach.
    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("custodian_archive_{name}_{}_{}", std::process::id(), crate::runs::stamp().replace([':', '.'], "")));
        let put = |rel: &str, text: &str| {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        };
        put("Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\"]\n");
        put("Cargo.lock", "# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 4\n\n[[package]]\nname = \"fixture_app\"\nversion = \"0.1.0\"\ndependencies = [\n \"fixture_mid\",\n]\n\n[[package]]\nname = \"fixture_leaf\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"fixture_mid\"\nversion = \"0.1.0\"\ndependencies = [\n \"fixture_leaf\",\n]\n");
        put("README.md", "# fixture\n");
        put("docs/delivery-schema.txt", "schema v1\n");
        put("crates/fixture_app/Cargo.toml", "[package]\nname = \"fixture_app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nfixture_mid = { path = \"../fixture_mid\" }\n");
        put("crates/fixture_app/README.md", "# app\n");
        put("crates/fixture_app/src/lib.rs", "pub fn app() -> u32 {\n    fixture_mid::mid() + 1\n}\n");
        put("crates/fixture_mid/Cargo.toml", "[package]\nname = \"fixture_mid\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nfixture_leaf = { path = \"../fixture_leaf\" }\n");
        put("crates/fixture_mid/README.md", "# mid\n");
        put("crates/fixture_mid/src/lib.rs", "pub fn mid() -> u32 {\n    fixture_leaf::leaf() + 1\n}\n");
        put("crates/fixture_leaf/Cargo.toml", "[package]\nname = \"fixture_leaf\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        put("crates/fixture_leaf/README.md", "# leaf\n");
        put("crates/fixture_leaf/src/lib.rs", "pub fn leaf() -> u32 {\n    1\n}\n");
        root
    }

    fn recursive_options() -> PlanOptions {
        PlanOptions { include_workspace_dependencies: true, attachments: ["Cargo.toml", "Cargo.lock", "README.md", "docs/delivery-schema.txt"].iter().map(PathBuf::from).collect() }
    }

    fn members(root: &Path) -> Members {
        let mut ran = Vec::new();
        let m = Members::read(root, &mut ran).unwrap();
        assert_eq!(ran.len(), 1, "one declared cargo line: {ran:?}");
        assert!(ran[0].line.starts_with("cargo metadata --manifest-path ") && ran[0].line.ends_with("--format-version 1 --no-deps"), "{}", ran[0].line);
        assert_eq!(ran[0].code, Some(0));
        m
    }

    #[test]
    fn members_come_from_the_declared_cargo_metadata_line_with_their_path_dependencies() {
        let root = fixture("members");
        let m = members(&root);
        assert_eq!(m.packages.keys().cloned().collect::<Vec<_>>(), ["fixture_app", "fixture_leaf", "fixture_mid"]);
        assert_eq!(m.packages["fixture_app"].local_dependencies, ["fixture_mid"]);
        assert_eq!(m.packages["fixture_mid"].local_dependencies, ["fixture_leaf"]);
        assert!(m.packages["fixture_leaf"].local_dependencies.is_empty());
        assert_eq!(m.by_dir(&root.join("crates/fixture_mid")).map(|p| p.name.as_str()), Some("fixture_mid"));
    }

    #[test]
    fn dependency_gaps_close_as_crates_are_added_by_hand() {
        let m = members(&fixture("gaps"));
        let app_only = m.plan(&["fixture_app".to_owned()]).unwrap();
        assert_eq!(app_only.gaps, [DependencyGap { from: "fixture_app".into(), dependency: "fixture_mid".into() }]);
        let app_and_mid = m.plan(&["fixture_app".to_owned(), "fixture_mid".to_owned()]).unwrap();
        assert_eq!(app_and_mid.gaps, [DependencyGap { from: "fixture_mid".into(), dependency: "fixture_leaf".into() }]);
        assert_eq!(m.plan(&[]).unwrap_err().to_string(), "[CUST0130] the archive job is refused: a route that is not a workspace crate, an attached file outside the root or missing, a duplicate archive path, or an output that exists: at least one crate must be selected");
        assert!(m.plan(&["nope".to_owned()]).unwrap_err().to_string().contains("unknown workspace crate: nope"));
    }

    #[test]
    fn the_closure_records_why_each_crate_is_there_and_the_attachments_their_place() {
        let m = members(&fixture("closure"));
        let plan = m.plan_with_options(&["fixture_app".to_owned()], &recursive_options()).unwrap();
        assert!(plan.gaps.is_empty());
        assert_eq!(plan.explicit, ["fixture_app"]);
        let by = |n: &str| plan.crates.iter().find(|c| c.name == n).unwrap().clone();
        assert_eq!(by("fixture_app"), CrateProvenance { name: "fixture_app".into(), kind: CrateInclusionKind::Explicit, required_by: vec![] });
        assert_eq!(by("fixture_mid"), CrateProvenance { name: "fixture_mid".into(), kind: CrateInclusionKind::Dependency, required_by: vec!["fixture_app".into()] });
        assert_eq!(by("fixture_leaf"), CrateProvenance { name: "fixture_leaf".into(), kind: CrateInclusionKind::Dependency, required_by: vec!["fixture_mid".into()] });
        assert_eq!(plan.attachments.len(), 4);
        assert!(plan.attachments.iter().any(|a| a.source == "Cargo.toml" && a.archive_path == "workspace/Cargo.toml"));
        assert!(plan.attachments.iter().any(|a| a.source == "docs/delivery-schema.txt" && a.archive_path == "workspace/docs/delivery-schema.txt"));
        assert_eq!(plan.files.len(), 13);
        let f = plan.files.iter().find(|f| f.path == "crates/fixture_mid/Cargo.toml").unwrap();
        assert_eq!(f.source, "crates/fixture_mid/Cargo.toml", "every included crate's manifest, with its source named");
        assert!(plan.files.iter().any(|f| f.path == "workspace/Cargo.lock") && plan.files.iter().any(|f| f.path == "workspace/README.md"));
        let manifest = String::from_utf8(plan.manifest_bytes()).unwrap();
        assert!(manifest.starts_with("schema\tcustodian.archive.v1\n"), "{manifest}");
        assert!(manifest.contains("crate\texplicit\tfixture_app\n") && manifest.contains("crate\tdependency\tfixture_mid\tfixture_app\n") && manifest.contains("crate\tdependency\tfixture_leaf\tfixture_mid\n"), "{manifest}");
        assert!(manifest.contains("attached\tdocs/delivery-schema.txt\tworkspace/docs/delivery-schema.txt\n"), "{manifest}");
        assert_eq!(manifest.lines().filter(|l| l.starts_with("file\tsha256:")).count(), 13, "{manifest}");
    }

    #[test]
    fn an_unsafe_attachment_is_refused() {
        let m = members(&fixture("unsafe"));
        let options = PlanOptions { include_workspace_dependencies: true, attachments: vec![PathBuf::from("../outside.txt")] };
        let e = m.plan_with_options(&["fixture_app".to_owned()], &options).unwrap_err().to_string();
        assert!(e.starts_with("[CUST0130]") && e.contains("unsafe archive path rejected: ../outside.txt"), "{e}");
        let options = PlanOptions { include_workspace_dependencies: true, attachments: vec![PathBuf::from("docs/missing.txt")] };
        assert!(m.plan_with_options(&["fixture_app".to_owned()], &options).unwrap_err().to_string().contains("cannot inspect attached file"));
    }

    #[test]
    fn the_build_is_byte_for_byte_repeatable_never_overwrites_and_reads_back() {
        let root = fixture("build");
        let m = members(&root);
        let plan = m.plan_with_options(&["fixture_app".to_owned()], &recursive_options()).unwrap();
        let first = root.join("out/a/fixture.zip");
        let second = root.join("out/b/fixture.zip");
        let d1 = plan.build_zip(&first).unwrap();
        let d2 = plan.build_zip(&second).unwrap();
        assert_eq!(d1, d2);
        assert!(d1.starts_with("sha256:"), "the kit's digest: {d1}");
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
        assert!(!root.join("out/a/fixture.zip.partial").exists(), "the partial file is renamed into place");
        let e = plan.build_zip(&first).unwrap_err().to_string();
        assert!(e.contains("refusing to overwrite"), "{e}");
        let report = verify_zip(&first).unwrap();
        assert_eq!(report, VerifyReport { files: 13, selected: vec!["fixture_app".into(), "fixture_leaf".into(), "fixture_mid".into()] });
        // a byte flipped in a payload: the digest no longer matches
        let mut bytes = fs::read(&first).unwrap();
        let at = bytes.windows(7).position(|w| w == b"# leaf\n").unwrap();
        bytes[at + 2] = b'L';
        let broken = root.join("out/broken.zip");
        fs::write(&broken, &bytes).unwrap();
        let e = verify_zip(&broken).unwrap_err().to_string();
        assert!(e.starts_with("[CUST0132]") && e.contains("CRC mismatch for crates/fixture_leaf/README.md"), "{e}");
    }

    /// The line's body over the fixture: routes resolved through the declared table, the closure, the archive written
    /// and read back; a gap is status 1; a route that is not a crate, or a path a job did not declare, is refused.
    #[test]
    fn exec_archive_takes_routes_and_leaves_an_archive_with_its_summary() {
        let root = fixture("exec");
        let out = root.join("delivery/fixture.zip");
        let job = ArchiveJob { root: root.clone(), routes: vec!["crate:fixture_app".into()], attach: vec!["path:Cargo.toml".into(), "path:docs/delivery-schema.txt".into(), "crate:fixture_leaf/README.md".into()], workspace_deps: true, reveal: false, out: Some(out.clone()), how: Resolving::Free };
        let s = exec_archive(&job).unwrap();
        assert_eq!((s.code(), s.files, s.crates.len(), s.gaps.len()), (0, 12, 3, 0));
        assert_eq!(s.attachments.iter().map(|a| a.source.as_str()).collect::<Vec<_>>(), ["Cargo.toml", "crates/fixture_leaf/README.md", "docs/delivery-schema.txt"]);
        assert_eq!(s.archive.as_deref(), Some(out.to_str().unwrap()));
        assert_eq!(s.verified.as_ref().map(|v| v.files), Some(12));
        assert_eq!(s.cargo.len(), 1);
        assert!(s.revealed.is_empty());
        assert!(out.is_file());
        let round: ArchiveSummary = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(round, s);
        // the plan only, a gap: status 1 and nothing written
        let check = ArchiveJob { routes: vec!["crate:fixture_app".into()], attach: vec![], workspace_deps: false, out: None, ..job.clone() };
        let s = exec_archive(&check).unwrap();
        assert_eq!((s.code(), s.gaps.len(), s.archive.is_none()), (1, 1, true));
        // a file route is not a crate; an undeclared path inside a job is refused by the table
        let e = exec_archive(&ArchiveJob { routes: vec!["crate:fixture_app/src/lib.rs".into()], ..check.clone() }).unwrap_err().to_string();
        assert!(e.contains("is not a crate: route"), "{e}");
        let e = exec_archive(&ArchiveJob { attach: vec!["path:README.md".into()], how: Resolving::Job, ..check.clone() }).unwrap_err().to_string();
        assert!(e.starts_with("[CUST0023]"), "{e}");
    }
}
