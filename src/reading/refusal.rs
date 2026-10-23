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
    #[liaise(code = 29, msg = "a 1o document does not parse, does not match its schema, or does not read into its form")]
    OneoInvalid,
    #[liaise(code = 30, msg = "a value cannot be written in 1o (core-1o reads no negative numbers)")]
    OneoUnwritable,
    #[liaise(code = 31, msg = "a bounded job's touched lines are not all covered by its transform receipts: the agent wrote code by hand")]
    BoundedUncovered,
    #[liaise(code = 32, msg = "the change spec is not a change spec (a field is missing, unknown or the wrong type, or its operation lacks what it needs)")]
    ChangeInvalid,
    #[liaise(code = 33, msg = "a code-changing step has no preflight receipt of this spec over this code that authorized it")]
    NoPreflight,
    #[liaise(code = 34, msg = "the preflight did not authorize the change")]
    NotAuthorized,
    #[liaise(code = 35, msg = "a linkage record is not a linkage record, or a declaration it binds does not resolve")]
    LinkageInvalid,
    #[liaise(code = 36, msg = "no such decision is open")]
    NoDecision,
    #[liaise(code = 37, msg = "a bounded job needs code-passenger on PATH: it is the writer")]
    NoWriter,
    #[liaise(code = 38, msg = "the plan has no step of that number")]
    NoPlanStep,
    #[liaise(code = 39, msg = "the reading reads declarations and none are named: no declared_in source in the spec, and no default in .custodian/config/declared.toml")]
    NoDeclaredSource,
    #[liaise(code = 40, msg = "an attribute macro rewrites the item, and the operation does not know how: it writes the item as declared, the macro does not leave it so")]
    MacroRewrites,
    #[liaise(code = 41, msg = "a file the job touched is neither code a transform wrote nor a file the job allows (allow_file)")]
    FileNotAllowed,
    #[liaise(code = 42, msg = "the battery is under 20% and not charging: no cargo line starts on a dying battery (plug the laptop in and run it again; the job stays open)")]
    LowBattery,
    #[liaise(code = 43, msg = "the step would run past the job's budget: refused before it starts (the job stays open)")]
    OverBudget,
    #[liaise(code = 44, msg = "the target names more than one function (two impls on one path): name the impl in its route, `path[impl Trait for Type]`")]
    AmbiguousTarget,
    #[liaise(code = 45, msg = "two receipt folders number receipts the same: merge them with `custodian receipts merge <other>`, which renumbers by union")]
    ReceiptCollision,
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
    #[liaise(code = 120, msg = "the exercise's options are refused: a word that is not an operation or a unit kind, a number that is not one, or a rule of the exercise word broken")]
    ExerciseRefused,
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

/// A history crate's refusal as the kit's: where the kit refused the same thing before the machinery moved out
/// (the journal, a write, a reading that cannot be read back, a missing input), the kit's own code and sentence
/// with the history's detail, so its refusals read as they did; any other keeps the history's own code.
macro_rules! from_history {
    ($($e:ty => { $($from:path => $to:ident),* $(,)? })*) => {$(
        impl From<$e> for CustodianError {
            fn from(e: $e) -> Self {
                #[allow(unreachable_patterns)]
                match e.refusal {
                    $($from => ReadingRefusal::$to.refuse(e.detail),)*
                    other => CustodianError::Reading { code: other.code(), message: format!("{}: {}", other.message(), e.detail) },
                }
            }
        }

        impl From<$e> for crate::api::Refusal {
            fn from(e: $e) -> Self {
                CustodianError::from(e).into()
            }
        }
    )*};
}

from_history! {
    receipt_history::Error => { receipt_history::Refusal::Journal => Journal, receipt_history::Refusal::Write => Write }
    revision_history::Error => { revision_history::Refusal::Write => Write, revision_history::Refusal::Unreadable => ReadingUnreadable }
    tree_history::Error => { tree_history::Refusal::Write => Write, tree_history::Refusal::Unreadable => ReadingUnreadable, tree_history::Refusal::NotCounts => MissingInput }
    change_history::Error => {
        change_history::Refusal::Unreadable => ReadingUnreadable,
        change_history::Refusal::NotCounts => MissingInput,
        change_history::Refusal::MissingInput => MissingInput,
        change_history::Refusal::Write => Write,
    }
    diff_history::Error => { diff_history::Refusal::Unreadable => ReadingUnreadable }
}
