use std::ffi::c_uchar;
use std::ffi::c_ulong;
use std::ffi::c_ushort;
use std::ffi::c_void;
use std::path::Path;
use std::path::PathBuf;

use bytes::BufMut;
use bytes::BytesMut;
use cimfs_sys::ACCESS_SYSTEM_SECURITY;
use cimfs_sys::CIMFS_IMAGE_HANDLE__;
use cimfs_sys::PROCESS_TRUST_LABEL_SECURITY_INFORMATION;
use windows::core::Result;
use windows::core::HRESULT;
use windows::core::HSTRING;
use windows::Win32::Foundation::*;
use windows::Win32::Security::Authorization::GetSecurityInfo;
use windows::Win32::Security::Authorization::SE_FILE_OBJECT;
use windows::Win32::Security::DACL_SECURITY_INFORMATION;
use windows::Win32::Security::*;
use windows::Win32::Storage::FileSystem::CreateFileW;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::IO::DeviceIoControl;

use crate::raw::CIMFS_IMAGE_HANDLE;
use crate::raw::FSCTL_GET_REPARSE_POINT;

use tracing::*;

/// Struct providing wrappers around CimFS image apis,
///
pub struct Image {
    /// Name of this image,
    ///
    name: String,
    /// Root directory containing this image,
    ///
    root_folder: PathBuf,
    /// Cimfs image handle,
    ///
    _image_handle: CIMFS_IMAGE_HANDLE,
}

impl Image {
    /// Creates a new image w/ root_folder containing the images and a name for this image,
    ///
    pub fn new(root_folder: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root_folder: root_folder.into(),
            _image_handle: std::ptr::null_mut(),
        }
    }

    /// Creates the current image,
    ///
    pub fn create(&mut self, existing: Option<&str>) -> Result<()> {
        unsafe {
            use crate::raw::CimCreateImage;

            // Prepare parameters
            let root = HSTRING::from(self.root_folder.as_os_str());
            let file_name = HSTRING::from(self.name.as_str());
            let existing_name = if let Some(existing) = existing.as_ref() {
                HSTRING::from(*existing).as_ptr()
            } else {
                std::ptr::null()
            };
            let mut handle = std::ptr::null_mut();

            let result: HRESULT = HRESULT(CimCreateImage(
                root.as_ptr(),
                existing_name,
                file_name.as_ptr(),
                std::ptr::addr_of_mut!(handle),
            ));
            if result.is_err() {
                return Err(result.into());
            }

            trace!("Got image handle -- {:?}", handle);
            self._image_handle = handle;
        }

        Ok(())
    }

    /// Adds a file to the image at the relative path in the image, copying data from src,
    ///
    /// Returns a writer if a stream was successfully created,
    ///
    pub fn create_file(
        &mut self,
        relative_path: impl AsRef<str>,
        src: impl AsRef<Path>,
    ) -> Result<()> {
        trace!(
            "Creating cim file for {:?} at {:?}",
            src.as_ref(),
            relative_path.as_ref()
        );
        unsafe {
            trace!("image handle -- {:?}", self._image_handle);
            use crate::raw::CimCloseStream;
            use crate::raw::CimCreateFile;
            use crate::raw::CimWriteStream;
            use crate::raw::CIMFS_FILE_METADATA;

            // Setup parameters
            let relative_path = HSTRING::from(relative_path.as_ref());
            trace!("Getting handle for {:?}", src.as_ref());
            let handle = CreateFileW(
                &HSTRING::from(src.as_ref().as_os_str()).into(),
                (GENERIC_READ | GENERIC_ACCESS_RIGHTS(ACCESS_SYSTEM_SECURITY)).0,
                FILE_SHARE_READ,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )?;

            let mut basic_info = FILE_BASIC_INFO::default();

            GetFileInformationByHandleEx(
                handle,
                FileBasicInfo,
                std::ptr::addr_of_mut!(basic_info) as *mut c_void,
                std::mem::size_of_val(&basic_info) as u32,
            )
            .ok()?;

            trace!("Got file info -- {:#?}", basic_info);

            let mut metadata = CIMFS_FILE_METADATA {
                Attributes: basic_info.FileAttributes,
                CreationTime: crate::raw::to_large_int(basic_info.CreationTime),
                LastWriteTime: crate::raw::to_large_int(basic_info.LastWriteTime),
                ChangeTime: crate::raw::to_large_int(basic_info.ChangeTime),
                LastAccessTime: crate::raw::to_large_int(basic_info.LastAccessTime),
                FileSize: 0,
                SecurityDescriptorBuffer: std::ptr::null(),
                SecurityDescriptorSize: 0,
                ReparseDataBuffer: std::ptr::null(),
                ReparseDataSize: 0,
                EaBuffer: std::ptr::null(),
                EaBufferSize: 0,
            };

            if basic_info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
                metadata.FileSize = 0;
            } else {
                let mut file_size = 0;
                GetFileSizeEx(handle, std::ptr::addr_of_mut!(file_size)).ok()?;
                metadata.FileSize = file_size;
            }
            trace!("Getting file size -- {}", metadata.FileSize);

            let mut buf = BytesMut::with_capacity(MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize);
            let mut bytes: c_ulong = 0;

            if basic_info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
                trace!("Getting reparse data");
                buf.set_len(MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize);

                DeviceIoControl(
                    handle,
                    FSCTL_GET_REPARSE_POINT,
                    None,
                    0,
                    Some(buf.as_mut_ptr() as *mut c_void),
                    buf.len() as c_ulong,
                    Some(std::ptr::addr_of_mut!(bytes)),
                    None,
                )
                .ok()?;

                metadata.ReparseDataBuffer = buf.freeze().as_ptr() as *const c_void;
                metadata.ReparseDataSize = bytes;
            }

            let sec_info = DACL_SECURITY_INFORMATION
                | LABEL_SECURITY_INFORMATION
                | GROUP_SECURITY_INFORMATION
                | OWNER_SECURITY_INFORMATION
                | SACL_SECURITY_INFORMATION
                | OBJECT_SECURITY_INFORMATION(PROCESS_TRUST_LABEL_SECURITY_INFORMATION);
            let mut desc: PSECURITY_DESCRIPTOR = PSECURITY_DESCRIPTOR::default();

            GetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                sec_info.0,
                None,
                None,
                None,
                None,
                Some(std::ptr::addr_of_mut!(desc)),
            )
            .ok()?;

            if desc.is_invalid() {
                return Err(STATUS_UNSUCCESSFUL.into());
            }

            // metadata.SecurityDescriptorBuffer = std::ptr::addr_of!(desc) as *const c_void;
            // metadata.SecurityDescriptorSize = GetSecurityDescriptorLength(desc);
            // trace!(
            //     "Getting security information -- {:?} {}",
            //     desc,
            //     metadata.SecurityDescriptorSize
            // );

            // let ea = FILE_FULL_EA_INFORMATION::default();
            // metadata.EaBuffer = std::ptr::addr_of!(ea) as *const c_void;
            // metadata.EaBufferSize = std::mem::size_of_val(&ea) as u32;

            let path = relative_path.as_wide();
            let path = path.as_ptr();
            let metadata_p = std::ptr::addr_of!(metadata);
            trace!(
                "Creating file and getting stream handle, {:?} {:?} {:?} {:?}",
                relative_path,
                self._image_handle.is_null(),
                path.is_null(),
                metadata_p.is_null(),
            );
            let mut stream_handle = std::ptr::null_mut();

            let result = HRESULT(CimCreateFile(
                self._image_handle,
                path,
                metadata_p,
                std::ptr::addr_of_mut!(stream_handle),
            ));

            trace!(
                "Stream handle result {:?} -- stream_handle_is_null -- {}",
                result,
                stream_handle.is_null()
            );

            result.ok()?;

            let mut buffer = BytesMut::with_capacity(65536);
            buffer.set_len(65536);

            let mut total = 0;
            loop {
                trace!("Starting read");
                let mut read = 0;
                ReadFile(
                    handle,
                    Some(buffer.as_mut_ptr() as *mut c_void),
                    buffer.len() as u32,
                    Some(std::ptr::addr_of_mut!(read)),
                    None,
                )
                .ok()?;

                total += read;

                HRESULT(CimWriteStream(
                    stream_handle,
                    buffer.as_ptr() as *const c_void,
                    read,
                ))
                .ok()?;
                
                trace!("Read {read} bytes to buffer");
                if read == 0 {
                    break;
                }
                
                trace!("Wrote to stream");
                buffer.truncate(0);
                buffer.set_len(65536);
            }

            trace!("Closing stream - total written {}", total);
            CimCloseStream(stream_handle);
            stream_handle.drop_in_place();

            CloseHandle(handle).ok()?;
        }

        Ok(())
    }

    /// Commits the image,
    ///
    pub fn commit(&mut self) -> Result<()> {
        trace!("Committing image");
        unsafe {
            use crate::raw::CimCommitImage;

            HRESULT(CimCommitImage(self._image_handle)).ok()?;
        }

        Ok(())
    }
}

#[repr(C)]
#[derive(Default)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
struct FILE_FULL_EA_INFORMATION {
    NextEntryOffset: c_ulong,
    Flags: c_ulong,
    EaNameLength: c_uchar,
    EaValueLength: c_ushort,
    EaName: [c_uchar; 1],
}
