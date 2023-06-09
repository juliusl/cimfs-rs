use std::collections::BTreeSet;
use std::path::PathBuf;
use windows::Win32::Foundation::E_INVALIDARG;
use windows::core::Error;
use tracing::trace;

/// Struct containing data on the object being added to a CIM image,
///
#[derive(Debug, PartialEq, PartialOrd, Eq, Ord)]
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

    /// Resolves the relative path to use for this object, and returns a set of ancestors required to add this object,
    ///
    /// If the relative_path is not set, it will be interpreted from the src path.
    ///
    pub fn resolve_relative_path(&mut self, parse_ancestors: bool) -> Result<BTreeSet<Object>, Error> {
        let mut ancestors = BTreeSet::new();
        if self.relative_path.as_os_str().is_empty() {
            self.src
                .canonicalize()
                .map_err(|e| Error::new(E_INVALIDARG, format!("{e}").into()))?;
            let mut relative_path = PathBuf::new();

            for c in self.src.components() {
                trace!("{:?}", c);
                match c {
                    std::path::Component::Prefix(prefix) => match prefix.kind() {
                        std::path::Prefix::Verbatim(p) => {
                            relative_path = relative_path.join(p);
                        }
                        std::path::Prefix::VerbatimUNC(_, share)
                        | std::path::Prefix::UNC(_, share) => {
                            relative_path = relative_path.join(share);
                        }
                        std::path::Prefix::VerbatimDisk(_)
                        | std::path::Prefix::DeviceNS(_)
                        | std::path::Prefix::Disk(_) => {
                        }
                    },
                    // Treat all of these cases as a root path
                    std::path::Component::RootDir
                    | std::path::Component::CurDir
                    | std::path::Component::ParentDir => {
                    }
                    std::path::Component::Normal(p) => {
                        relative_path = relative_path.join(p);
                    }
                }
            }

            self.relative_path = relative_path;
            if parse_ancestors {
                for a in self.relative_path.ancestors() {
                    trace!("ancestor -- {:?}", a);
                    if !a.exists() || a.is_file() {
                        continue;
                    }

                    let mut a = Object::new(a);
                    a.resolve_relative_path(false)?;
                    ancestors.insert(a);
                }
            }
        }

        Ok(ancestors)
    }

    /// Returns a result containing the relative path to use in the cim for this object,
    ///
    /// If the resolve_relative_path() hasn't been called yet, this function will retrun an error.
    ///
    pub fn get_relative_path(&self) -> Result<&PathBuf, Error> {
        if self.relative_path.as_os_str().is_empty() {
            Err(Error::new(
                E_INVALIDARG,
                "Object's relative path hasn't been resolved".into(),
            ))
        } else {
            Ok(&self.relative_path)
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

#[allow(unused_imports)]
mod tests {
    use super::Object;
    #[test]
    #[tracing_test::traced_test]
    fn test_resolve() {
        let mut t = Object::new("src/bin/cimutil.rs");

        let ancestors = t.resolve_relative_path(true);
        println!("{:#?}", ancestors);
    }
}