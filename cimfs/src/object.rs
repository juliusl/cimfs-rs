use std::path::PathBuf;
use std::ffi::OsStr;
use windows::Win32::Foundation::E_INVALIDARG;
use windows::core::Error;
use tracing::trace;

/// Struct containing data on the object being added to a CIM image,
///
pub struct Object {
    /// Relative path to use in the CIM image,
    ///
    relative_path: PathBuf,
    /// Path to the src object,
    ///
    src: PathBuf,
}

impl Object {
    /// Creates a new object from a src path w/ an empty relative path,
    ///
    pub fn new(src: impl Into<PathBuf>) -> Self {
        Self {
            src: src.into(),
            relative_path: PathBuf::new(),
        }
    }

    /// Returns the relative path to use in the CIM image,
    ///
    /// If the relative_path is not set, it will be interpreted from the src path.
    ///
    /// If the src path starts with '.' or '..', it will be treated as the root ('\\').
    ///
    pub fn resolve_relative_path(&mut self) -> Result<(), Error> {
        if !self.relative_path.as_os_str().is_empty() {
            Ok(())
        } else {
            self.src
                .canonicalize()
                .map_err(|e| Error::new(E_INVALIDARG, format!("{e}").into()))?;
            let mut relative_path = PathBuf::new();

            for c in self.src.components() {
                trace!("{:?}", c);
                match c {
                    std::path::Component::Prefix(prefix) => match prefix.kind() {
                        std::path::Prefix::Verbatim(p) => {
                            relative_path = relative_path.join("\\").join(p);
                        }
                        std::path::Prefix::VerbatimUNC(_, share)
                        | std::path::Prefix::UNC(_, share) => {
                            relative_path = relative_path.join("\\").join(share);
                        }
                        std::path::Prefix::VerbatimDisk(_)
                        | std::path::Prefix::DeviceNS(_)
                        | std::path::Prefix::Disk(_) => {
                            relative_path = relative_path.join("\\");
                        }
                    },
                    // Treat all of these cases as a root path
                    std::path::Component::RootDir
                    | std::path::Component::CurDir
                    | std::path::Component::ParentDir => {
                        relative_path = relative_path.join("\\");
                    }
                    std::path::Component::Normal(p) => {
                        relative_path = relative_path.join(p);
                    }
                }
            }

            self.relative_path = relative_path;
            Ok(())
        }
    }

    /// Returns a result containing the relative path to use in the cim for this object,
    ///
    /// If the resolve_relative_path() hasn't been called yet, this function will retrun an error.
    ///
    pub fn get_relative_path(&self) -> Result<&OsStr, Error> {
        if self.relative_path.as_os_str().is_empty() {
            Err(Error::new(
                E_INVALIDARG,
                "Object's relative path hasn't been resolved".into(),
            ))
        } else {
            Ok(self.relative_path.as_os_str())
        }
    }

    /// Returns the fully qualified path to the src object,
    ///
    pub fn get_src_path(&self) -> Result<PathBuf, Error> {
        self.src
            .canonicalize()
            .map_err(|e| Error::new(E_INVALIDARG, format!("{e}").into()))
    }
}
