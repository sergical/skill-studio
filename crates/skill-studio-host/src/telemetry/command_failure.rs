//! Classifies command errors without retaining their text.

use std::io;

use skill_studio_core::error::ErrorCode;

/// A fixed code for a failed command or operation.
#[derive(Clone, Copy, Debug)]
pub enum CommandFailure {
    /// The shared error vocabulary used by core operations.
    Core(ErrorCode),
    /// All available process or system file descriptors are in use.
    TooManyOpenFiles,
    /// The OS refused access.
    PermissionDenied,
    /// A file or resource was missing.
    NotFound,
    /// An I/O operation timed out.
    TimedOut,
    /// An I/O failure outside the known kinds.
    OtherIo,
    /// A legacy command returned an unclassified string.
    Unknown,
}

impl CommandFailure {
    /// Classifies an I/O error using its OS code or error kind.
    pub fn from_io(error: &io::Error) -> Self {
        if error.raw_os_error().is_some_and(is_file_limit) {
            return Self::TooManyOpenFiles;
        }
        match error.kind() {
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::TimedOut => Self::TimedOut,
            _ => match Self::from_legacy(&error.to_string()) {
                Self::Unknown => Self::OtherIo,
                failure => failure,
            },
        }
    }

    /// Classifies a legacy string using only Rust's terminal OS error number.
    pub fn from_legacy(error: &str) -> Self {
        let errno = error
            .strip_suffix(')')
            .and_then(|text| text.rsplit_once(" (os error "))
            .and_then(|(_, number)| number.parse::<i32>().ok());
        match errno {
            Some(errno) if is_file_limit(errno) => Self::TooManyOpenFiles,
            Some(errno) => match io::Error::from_raw_os_error(errno).kind() {
                io::ErrorKind::PermissionDenied => Self::PermissionDenied,
                io::ErrorKind::NotFound => Self::NotFound,
                io::ErrorKind::TimedOut => Self::TimedOut,
                _ => Self::OtherIo,
            },
            None => Self::Unknown,
        }
    }

    /// Keeps the core code, specializing I/O when an OS cause survived.
    pub fn from_core(code: ErrorCode, cause: Option<&io::Error>, message: &str) -> Self {
        if code == ErrorCode::Io {
            let failure = cause.map_or_else(|| Self::from_legacy(message), Self::from_io);
            if !matches!(failure, Self::Unknown) {
                return failure;
            }
        }
        Self::Core(code)
    }

    /// Stable text suitable for a Sentry tag.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core(code) => code.as_str(),
            Self::TooManyOpenFiles => "too_many_open_files",
            Self::PermissionDenied => "permission_denied",
            Self::NotFound => "not_found",
            Self::TimedOut => "timed_out",
            Self::OtherIo => "other_io",
            Self::Unknown => "unknown",
        }
    }

    /// Only unexpected failures open an issue. Expected refusals stay in traces.
    pub(super) fn opens_issue(self) -> bool {
        match self {
            Self::Core(code) => match code {
                ErrorCode::InvalidRequest
                | ErrorCode::InvalidScope
                | ErrorCode::AmbiguousTarget
                | ErrorCode::Unsupported
                | ErrorCode::BackupExpired
                | ErrorCode::ScopeBusy
                | ErrorCode::StaleProposal
                | ErrorCode::DriftConflict
                | ErrorCode::OwnershipChanged
                | ErrorCode::AlreadyReverted
                | ErrorCode::Cancelled => false,
                ErrorCode::ExecutionFailed
                | ErrorCode::Io
                | ErrorCode::RecoveryPending
                | ErrorCode::Incomplete => true,
            },
            _ => true,
        }
    }
}

fn is_file_limit(errno: i32) -> bool {
    errno == rustix::io::Errno::MFILE.raw_os_error()
        || errno == rustix::io::Errno::NFILE.raw_os_error()
}
