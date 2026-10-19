//! Every refusal custodian gives, declared once as liaise codes: `[CUST0001]`
//! and its sentence. The code is what a script matches on; the sentence is
//! what a person reads; the detail after it says what was refused.

use crate::CustodianError;
use liaise::{Liaise, LiaiseCodes};

#[derive(Debug, Clone, Copy, PartialEq, Eq, LiaiseCodes)]
#[liaise(prefix = "CUST", domain = "custodian")]
pub enum ReadingRefusal {
    #[liaise(code = 1, msg = "the spec file cannot be read")]
    SpecUnreadable,
    #[liaise(code = 2, msg = "the spec is not a reading spec (a field is missing, unknown or the wrong type)")]
    SpecInvalid,
    #[liaise(code = 3, msg = "the spec's target is empty")]
    TargetMissing,
    #[liaise(code = 4, msg = "the project root has no Cargo.toml cargo can read")]
    NoWorkspace,
    #[liaise(code = 5, msg = "klepto could not scan the workspace")]
    Scan,
    #[liaise(code = 6, msg = "a declared_in file pattern is not a glob")]
    DeclaredGlob,
    #[liaise(code = 7, msg = "a declared_in file is not TOML")]
    DeclaredFile,
    #[liaise(code = 8, msg = "a declared_in key path is not a key path")]
    KeyPath,
    #[liaise(code = 9, msg = "a file custodian keeps cannot be written")]
    Write,
    #[liaise(code = 10, msg = "the target names no crate, directory or item of the scan")]
    TargetUnknown,
    #[liaise(code = 11, msg = "no initialized folder (.custodian) here or above; this needs one (custodian init)")]
    NotInitialized,
    #[liaise(code = 12, msg = "the journal cannot be read or written")]
    Journal,
    #[liaise(code = 13, msg = "a declared line was refused before it ran")]
    LineRefused,
    #[liaise(code = 14, msg = "a declared line could not be started")]
    LineNotStarted,
    #[liaise(code = 15, msg = "no such job")]
    NoJob,
    #[liaise(code = 16, msg = "the job is not in the state this needs")]
    JobState,
    #[liaise(code = 17, msg = "a reading the line wrote cannot be read back")]
    ReadingUnreadable,
    #[liaise(code = 18, msg = "nothing to compare: a spec needs two readings for a diff")]
    NoDiff,
    #[liaise(code = 19, msg = "the change is empty: affected_tests needs files, a git ref or items")]
    NoChange,
    #[liaise(code = 20, msg = "a threshold file is not a threshold file")]
    Thresholds,
    #[liaise(code = 21, msg = "a receipt or plan this needs is missing")]
    MissingInput,
    #[liaise(code = 22, msg = "the route is not declared: crate:<name>, decl:<name>, custodian:<part> or a declared path:<raw>")]
    UndeclaredRoute,
    #[liaise(code = 23, msg = "a job reads declared routes only: a walked path cannot be a job's target or declaration file")]
    WalkedPathInJob,
    #[liaise(code = 24, msg = "code_passenger has no line for this, and custodian never edits source or manifests itself")]
    NoWriterLine,
    #[liaise(code = 25, msg = "the program a declared line runs is not on this machine")]
    ProgramMissing,
    #[liaise(code = 26, msg = "another custodian run holds this project's .custodian folder; this run is refused (run it again when that one ends)")]
    WriterHeld,
    #[liaise(code = 27, msg = "a job is already running; close it first (custodian job close)")]
    JobRunning,
    #[liaise(code = 28, msg = "the serve socket answered with something that is not a receipt")]
    BadReply,
    #[liaise(code = 101, msg = "the bundle filters break the word's own rules")]
    FiltersRefused,
    #[liaise(code = 102, msg = "the route holds no file the filters keep")]
    NothingToBundle,
    #[liaise(code = 103, msg = "the maintenance registry cannot be read, so what is secret is not known")]
    Registry,
    #[liaise(code = 104, msg = "the level table has no `secret`, so nothing could be held; the bundle is refused rather than written open")]
    NoSecretLevel,
    #[liaise(code = 105, msg = "a held file could not be kept in a classified container")]
    Hold,
    #[liaise(code = 106, msg = "--no-feature names a feature the crate does not declare")]
    UnknownFeature,
    #[liaise(code = 107, msg = "bundles refused an entry")]
    Entry,
}

impl ReadingRefusal {
    /// The error this refusal raises, with what was refused.
    pub fn refuse(self, detail: impl std::fmt::Display) -> CustodianError {
        CustodianError::Reading { code: self.code(), message: format!("{}: {detail}", self.message()) }
    }

    /// The same refusal as the stable surface carries it (`api::Refusal`).
    pub fn refusal(self, detail: impl std::fmt::Display) -> crate::api::Refusal {
        self.refuse(detail).into()
    }
}
