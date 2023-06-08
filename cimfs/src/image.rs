use std::ffi::c_ulong;
use std::ffi::c_void;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::PathBuf;

use bytes::BytesMut;
use cimfs_sys::CimMountImage;
use cimfs_sys::CIM_MOUNT_IMAGE_FLAGS_CIM_MOUNT_IMAGE_NONE;
use cimfs_sys::_GUID;
use windows::core::Error;
use windows::core::PCWSTR;
use windows::core::Result;
use windows::core::GUID;
use windows::core::HRESULT;
use windows::core::HSTRING;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::CreateFileW;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::Rpc::UuidCreate;
use windows::Win32::System::IO::DeviceIoControl;

// TODO -- Used w/ security descriptors
// use cimfs_sys::ACCESS_SYSTEM_SECURITY;
// use bytes::BufMut;
// use cimfs_sys::CIMFS_IMAGE_HANDLE__;
// use cimfs_sys::PROCESS_TRUST_LABEL_SECURITY_INFORMATION;
// use windows::Win32::Security::Authorization::GetSecurityInfo;
// use windows::Win32::Security::Authorization::SE_FILE_OBJECT;
// use windows::Win32::Security::DACL_SECURITY_INFORMATION;
// use windows::Win32::Security::*;

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
    image_handle: Option<CimImageHandleWrapper>,
    /// Volume id,
    ///
    volume: Option<GUID>,
}

impl Image {
    /// Creates a new image w/ root_folder containing the images and a name for this image,
    ///
    pub fn new(root_folder: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root_folder: root_folder.into(),
            image_handle: None,
            volume: None,
        }
    }

    /// Sets the volume id, chainable
    /// 
    pub fn with_volume(mut self, volume: GUID) -> Self {
        self.volume = Some(volume);
        self
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
            self.image_handle = Some(CimImageHandleWrapper { handle });
        }

        Ok(())
    }

    /// Adds a file to the image at the relative path in the image, copying data from src,
    ///
    pub fn create_file(&mut self, relative_path: &OsStr, src: &OsStr) -> Result<()> {
        let relative_path = relative_path.to_str().unwrap();
        let src = src.to_str().unwrap().trim_start_matches("\\\\?\\");
        trace!("Creating cim file for {} at {}", src, relative_path,);

        if let Some(image_handle_wrapper) = self.image_handle.take() {
            unsafe {
                trace!("image handle -- {:?}", image_handle_wrapper);
                use crate::raw::CimCloseStream;
                use crate::raw::CimCreateFile;
                use crate::raw::CimWriteStream;
                use crate::raw::CIMFS_FILE_METADATA;

                // Setup parameters
                let relative_path = HSTRING::from(relative_path);
                trace!("Getting handle for {}", src);
                let handle = CreateFileW(
                    &HSTRING::from(src).into(),
                    GENERIC_READ.0, // (GENERIC_READ | GENERIC_ACCESS_RIGHTS(ACCESS_SYSTEM_SECURITY)).0,
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

                let mut is_dir = false;
                if basic_info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
                    metadata.FileSize = 0;
                    is_dir = true;
                } else {
                    let mut file_size = 0;
                    GetFileSizeEx(handle, std::ptr::addr_of_mut!(file_size)).ok()?;
                    metadata.FileSize = file_size;
                }
                trace!("Getting file size -- {}", metadata.FileSize);

                // Check for reparse point
                let mut buf = BytesMut::with_capacity(MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize);
                if basic_info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
                    trace!("Getting reparse data");
                    let mut bytes: c_ulong = 0;
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

                // TODO: There seems to be issues getting this to work w/ cimfs
                // let sec_info = DACL_SECURITY_INFORMATION
                //     | LABEL_SECURITY_INFORMATION
                //     | GROUP_SECURITY_INFORMATION
                //     | OWNER_SECURITY_INFORMATION
                //     | SACL_SECURITY_INFORMATION
                //     | OBJECT_SECURITY_INFORMATION(PROCESS_TRUST_LABEL_SECURITY_INFORMATION);
                // let mut desc: PSECURITY_DESCRIPTOR = PSECURITY_DESCRIPTOR::default();

                // GetSecurityInfo(
                //     handle,
                //     SE_FILE_OBJECT,
                //     sec_info.0,
                //     None,
                //     None,
                //     None,
                //     None,
                //     Some(std::ptr::addr_of_mut!(desc)),
                // )
                // .ok()?;

                // if desc.is_invalid() {
                //     return Err(STATUS_UNSUCCESSFUL.into());
                // }

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
                    image_handle_wrapper.handle.is_null(),
                    path.is_null(),
                    metadata_p.is_null(),
                );
                let mut stream_handle = std::ptr::null_mut();

                let result = HRESULT(CimCreateFile(
                    image_handle_wrapper.handle,
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

                if !is_dir {
                    trace!("Starting read");
                    let mut total = 0;
                    loop {
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
                }

                CimCloseStream(stream_handle);
                CloseHandle(handle).ok()?;

                // Restore the handle
                self.image_handle = Some(image_handle_wrapper);
            }
            Ok(())
        } else {
            Err(STATUS_UNSUCCESSFUL.into())
        }
    }

    /// Commits the image,
    ///
    pub fn commit(&mut self) -> Result<()> {
        trace!("Committing image");

        if let Some(image_handle) = self.image_handle.take() {
            unsafe {
                use crate::raw::CimCommitImage;

                HRESULT(CimCommitImage(image_handle.handle)).ok()?;
            }

            Ok(())
        } else {
            Err(STATUS_UNSUCCESSFUL.into())
        }
    }

    /// Mounts the image and returns the volume id GUID of the mounted volume,
    ///
    /// Will also cache the volume guid so that `set_mountpoint()` can be called subsequently
    ///
    pub fn mount(&mut self, volume_guid: Option<String>) -> Result<GUID> {
        let guid = if let Some(volume) = volume_guid {
            GUID::try_from(volume.as_str())
                .map_err(|_| Error::new(E_INVALIDARG, "Could not parse guid".into()))?
        } else if let Some(existing) = self.volume.take() {
            existing
        } else {
            unsafe {
                let mut guid = GUID::zeroed();

                let status = UuidCreate(std::ptr::addr_of_mut!(guid));
                if status.0 != 0 {
                    return Err(Error::new(E_FAIL, "Could not generate a new uuid".into()));
                }

                guid
            }
        };

        unsafe {
            trace!("Mounting image");
            HRESULT(CimMountImage(
                HSTRING::from(self.root_folder.as_os_str()).as_ptr(),
                HSTRING::from(self.name.as_str()).as_ptr(),
                CIM_MOUNT_IMAGE_FLAGS_CIM_MOUNT_IMAGE_NONE,
                std::ptr::addr_of!(guid) as *const _GUID,
            ))
            .ok()?;
        }

        self.volume = Some(guid);

        Ok(guid)
    }

    /// Sets the mountpoint for the mounted volume,
    /// 
    /// Returns an error if mount() was not called in the same process or with_volume() was not used.
    ///
    pub fn mount_volume(&self, mountpoint: impl Into<PathBuf>) -> Result<()> {
        if let Some(volume) = self.volume.as_ref() {
            unsafe {
                let volume_path = format!("\\\\?\\Volume{{{:?}}}\\", volume);
                let mut mountpoint = mountpoint.into();

                let mountpoint = mountpoint.as_mut_os_string();
                mountpoint.push(OsString::from("\\"));

                let mountpoint = HSTRING::from(mountpoint.as_os_str());
                let volume_path = HSTRING::from(volume_path);
                
                trace!("Trying to set mountpoint {} for {}", mountpoint.to_string(), volume_path.to_string());
                let mut mountpoint_term: Vec<u16> = vec![0; mountpoint.as_wide().len() + 1];
                mountpoint_term[..mountpoint.as_wide().len()].copy_from_slice(mountpoint.as_wide());
                mountpoint_term.push(0);

                let mut volume_path_term: Vec<u16> = vec![0; volume_path.as_wide().len() + 1];
                volume_path_term[..volume_path.as_wide().len()].copy_from_slice(volume_path.as_wide());
                volume_path_term.push(0);

                SetVolumeMountPointW(
                    PCWSTR(mountpoint_term.as_ptr()),
                    PCWSTR(volume_path_term.as_ptr()),
                )
                .ok()?;
            }

            Ok(())
        } else {
            Err(Error::new(E_NOINTERFACE, "A volume id does not exist in the cache, it's likely mount() or with_volume() have yet been called".into()))
        }
    }
}

/// Wrapper struct over the image handle so that it can be dropped in the case an error is returned while the handle is in-use
///
#[derive(Debug)]
struct CimImageHandleWrapper {
    handle: CIMFS_IMAGE_HANDLE,
}

impl Drop for CimImageHandleWrapper {
    fn drop(&mut self) {
        unsafe {
            crate::raw::CimCloseImage(self.handle);
        }
    }
}
