mod image;
mod object;

/// Module contains wrapper-types that add convenience api's.
/// 
pub mod api {
    pub use super::image::Image;
    pub use super::object::Object;
}

/// Module contains raw generated api's as well as utiltiies for working with the os.
///
pub mod raw {
    use std::ffi::c_ulong;
    pub use cimfs_sys::CimCloseImage;
    pub use cimfs_sys::CimCloseStream;
    pub use cimfs_sys::CimCommitImage;
    pub use cimfs_sys::CimCreateAlternateStream;
    pub use cimfs_sys::CimCreateFile;
    pub use cimfs_sys::CimCreateHardLink;
    pub use cimfs_sys::CimCreateImage;
    pub use cimfs_sys::CimDeletePath;
    pub use cimfs_sys::CimDismountImage;
    pub use cimfs_sys::CimMountImage;
    pub use cimfs_sys::CimWriteStream;
    pub use cimfs_sys::CIMFS_FILE_METADATA;
    pub use cimfs_sys::CIMFS_IMAGE_HANDLE;
    pub use cimfs_sys::CIMFS_STREAM_HANDLE;
    pub use cimfs_sys::CIM_MOUNT_IMAGE_FLAGS;
    pub use cimfs_sys::_GUID;

    use cimfs_sys::LARGE_INTEGER;

    /// Converts a large integer to the sys type,
    ///
    pub fn to_large_int(i: impl Into<i64>) -> LARGE_INTEGER {
        cimfs_sys::_LARGE_INTEGER { QuadPart: i.into() }
    }

    use cimfs_sys::FILE_ANY_ACCESS;
    use cimfs_sys::FILE_DEVICE_FILE_SYSTEM;
    use cimfs_sys::METHOD_BUFFERED;

    /// IOCTL for getting reparse point,
    ///
    pub const FSCTL_GET_REPARSE_POINT: c_ulong = ctl_code(
        FILE_DEVICE_FILE_SYSTEM,
        42,
        METHOD_BUFFERED,
        FILE_ANY_ACCESS,
    );

    /// Returns an ioctl code,
    ///
    /// Adapted from macro:
    ///
    /// ```cpp
    /// #define CTL_CODE(DeviceType, Function, Method, Access) ( ((DeviceType) << 16) | ((Access) << 14) | ((Function) << 2) | (Method) )
    /// ```
    ///
    const fn ctl_code(
        device_type: c_ulong,
        function: c_ulong,
        method: c_ulong,
        access: c_ulong,
    ) -> c_ulong {
        (device_type << 16) | (access << 14) | (function << 2) | method
    }
}

/// Utilities for environment setup,
/// 
pub mod util {
    use cimfs_sys::TOKEN_QUERY;
    use cimfs_sys::TOKEN_ADJUST_PRIVILEGES;
    use tracing::trace;
    use windows::core::Result;
    use windows::core::HSTRING;
    use windows::Win32::Security::TOKEN_PRIVILEGES_ATTRIBUTES;
    use windows::Win32::Security::TOKEN_PRIVILEGES;
    use windows::Win32::Security::TOKEN_ACCESS_MASK;
    use windows::Win32::Security::SE_PRIVILEGE_ENABLED_BY_DEFAULT;
    use windows::Win32::Security::SE_PRIVILEGE_ENABLED;
    use windows::Win32::Security::LookupPrivilegeValueW;
    use windows::Win32::Security::AdjustTokenPrivileges;
    use windows::Win32::System::Threading::*;
    use windows::Win32::Foundation::HANDLE;

    /// Setup privileges,
    ///
    pub fn setup_privileges() -> Result<()> {
        use windows::Win32::Security::SE_SECURITY_NAME;
        use windows::Win32::Security::SE_BACKUP_NAME;

        unsafe {
            let previous = toggle_privilege(SE_SECURITY_NAME.to_string()?, true)?;
            trace!(
                previous,
                "toggled privilege {:?}",
                SE_SECURITY_NAME.to_string()
            );
            let previous = toggle_privilege(SE_BACKUP_NAME.to_string()?, true)?;
            trace!(
                previous,
                "toggled privilege {:?}",
                SE_BACKUP_NAME.to_string()
            );
        }

        Ok(())
    }

    /// Toggles privileges,
    ///
    pub unsafe fn toggle_privilege(name: impl AsRef<str>, enable: bool) -> Result<bool> {
        let mut token = HANDLE::default();

        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ACCESS_MASK(TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY),
            std::ptr::addr_of_mut!(token),
        )
        .ok()?;

        let mut current_token_privileges = TOKEN_PRIVILEGES::default();
        let mut previous_token_privileges = TOKEN_PRIVILEGES::default();

        LookupPrivilegeValueW(
            None,
            &HSTRING::from(name.as_ref()),
            std::ptr::addr_of_mut!(current_token_privileges.Privileges[0].Luid),
        )
        .ok()?;

        current_token_privileges.PrivilegeCount = 1;
        current_token_privileges.Privileges[0].Attributes = if enable {
            SE_PRIVILEGE_ENABLED
        } else {
            TOKEN_PRIVILEGES_ATTRIBUTES(0)
        };

        let mut bytes = 0;
        AdjustTokenPrivileges(
            token,
            false,
            Some(std::ptr::addr_of!(current_token_privileges)),
            std::mem::size_of_val(&current_token_privileges) as u32,
            Some(std::ptr::addr_of_mut!(previous_token_privileges)),
            Some(std::ptr::addr_of_mut!(bytes)),
        )
        .ok()?;

        trace!("current - {:?}", current_token_privileges);
        trace!("previous - {:?} {}", previous_token_privileges, bytes);

        let previously_enabled: bool = (bytes
            == std::mem::size_of_val(&previous_token_privileges) as u32)
            && (previous_token_privileges.PrivilegeCount == 1)
            && (previous_token_privileges.Privileges[0].Attributes
                & (SE_PRIVILEGE_ENABLED | SE_PRIVILEGE_ENABLED_BY_DEFAULT))
                .0
                != 0;

        Ok(previously_enabled)
    }
}
