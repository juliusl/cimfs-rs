use clap::Args;
use clap::Parser;
use clap::Subcommand;
use std::path::PathBuf;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use windows::core::Error;
use windows::core::Result;
use windows::core::GUID;
use windows::core::HRESULT;
use windows::core::HSTRING;
use windows::Win32::Foundation::E_INVALIDARG;

use cimfs::api::*;
use cimfs::raw::_GUID;
use cimfs::raw::CimDismountImage;

/// Command line utility to work with CimFS on Windows
///
#[derive(Parser)]
#[command(name = "cimutil")]
struct CimUtil {
    /// Enables trace logging.
    ///
    /// This is equivalent to setting the environment variable RUST_LOG='cimutil=trace,cimfs=trace'.
    ///
    #[arg(long)]
    trace: bool,
    /// Sets the root path containing the cim images and data,
    ///
    #[arg(long, default_value_t=String::from("."))]
    root: String,

    #[command(subcommand)]
    command: CimFSCommands,
}

#[derive(Subcommand)]
enum CimFSCommands {
    /// Creates and builds a new CIM image,
    ///
    New(NewCimArgs),
    /// Create and builds a new CIM image based on a pre-existing image,
    ///
    Fork(ForkCimArgs),
    /// Mounts a cim image as a read-only volume,
    ///
    /// Prints the mounted volume path to stdout
    ///
    Mount(MountCimArgs),
    /// Dismounts a cim image by volume-id,
    ///
    /// You can locate the volume-id via `mountvol` or w/ `winobj.exe` from sys-internals,
    ///
    Dismount(DismountCimArgs),
}

/// Set of arguments for creating a new cim image.
///
/// The new image will be created in the directory specified by the `--root` argument.
/// If an existing image/file exists with the same name this command will fail.
///
#[derive(Args)]
struct NewCimArgs {
    /// Name of the cim image, ex. image.cim
    ///
    #[arg(long, short)]
    name: String,
    /// List of paths of objects to add to the new cim image,
    ///
    /// Objects can be:
    /// - Files
    /// - Directories
    ///
    /// The relative path in the image will be the relative path passed to this command. However, a path must be able to be canonicalized.
    ///
    /// For example,
    ///
    /// Passing `../src/file.txt` will be available in the cim image as `src/file.txt` however,
    /// before the file is queued the `..` will be expanded to a fully qualified path and if unsuccessful this command will fail.
    ///
    objects: Vec<String>,
}

/// Set of arguments for forking an existing cim image.
///
/// The forked image will be created in the directory specified by the `--root` argument.
/// The existing image this fork is based on must already exist in the root directory.
/// If an existing image exists with the name of the fork, this command will fail.
///
#[derive(Args)]
struct ForkCimArgs {
    /// Name of the existing cim, must exist in the same root directory. ex: existing.cim
    ///
    #[arg(long, short)]
    from: String,
    /// Name of the new cim based on the existing cim. ex: forked.cim
    ///
    /// This forked cim will be created in the same root directory.
    ///
    #[arg(long, short)]
    to: String,
    /// List of paths of objects to add to the new cim image, if a file existed in the previous image that file will be overwritten.
    ///
    /// Objects can be:
    /// - Files
    /// - Directories
    ///
    /// The relative path in the image will be the relative path passed to this command. However, a path must be able to be canonicalized.
    ///
    /// For example,
    ///
    /// Passing `../src/file.txt` will be available in the cim image as `src/file.txt` however,
    /// before the file is queued the `..` will be expanded to a fully qualified path and if unsuccessful this command will fail.
    ///
    objects: Vec<String>,
}

/// Arguments to mount a CimFS volume,
///
#[derive(Args)]
struct MountCimArgs {
    /// GUID to use for the mounted volume,
    ///
    /// If not set, a new GUID will be generated.
    ///
    #[arg(long, short)]
    volume: Option<String>,
    /// (Optional) Path to mount the volume after it is created,
    /// 
    /// See the `mountvol` windows command for restrictions.
    /// 
    /// A trailing slash will be added if one was not already added. 
    /// 
    /// **NOTE** If the path is occupied, setting the mount point will fail, but the volume will still be mounted.
    /// 
    #[arg(long, short)]
    mountvol: Option<String>,
    /// Image name to mount, ex. image.cim
    ///
    /// The image must exist in the directory specified by the `--root` argument.
    ///
    image: String,
}

/// Arguments to dismount a CimFS volume,
///
#[derive(Args)]
struct DismountCimArgs {
    /// Volume of the CimFS to dismount,
    ///
    /// Input can be in the following formats,
    /// - Volume{04522dcd-f383-4f1c-aea6-af8f93e020d5} 
    /// - {04522dcd-f383-4f1c-aea6-af8f93e020d5}
    /// - 04522dcd-f383-4f1c-aea6-af8f93e020d5
    /// 
    volume: String,
}

fn main() -> Result<()> {
    // Parse command line
    //
    let parser = CimUtil::parse();

    // Enable logging
    //
    enable_logging(parser.trace);

    let root = parser.root;
    let mut root = PathBuf::from(root);

    // Validate the root directory argument
    //
    if let CimFSCommands::Dismount(_) = &parser.command {
        trace!("Skipping root check");
    } else {
        root = root.canonicalize().map_err(|e| {
            error!(
                error = format!("{e}"),
                "Encountered error when canonicalizing {:?}", root
            );
            windows::core::Error::new(
                E_INVALIDARG,
                HSTRING::from("Could not canonicalize path to root directory"),
            )
        })?;
    }


    match parser.command {
        CimFSCommands::New(args) => {
            // Setup arguments before starting anything
            let name = args.name;
            if name.is_empty() {
                return Err(Error::new(E_INVALIDARG, HSTRING::from("Name was empty")));
            }

            trace!("Parsing objects to add");
            // TODO: Add a way to add this from a file schema, oci-manifest, tar, etc.
            let objects = parse_objects_from_args(args.objects)?;

            info!("Creating new CIM at: {:?}", root.join(&name));
            let mut image = Image::new(root, name);

            image.create(None)?;

            for o in objects {
                let relative_path = o.get_relative_path()?;
                let src_path = o.get_src_path()?;

                info!("Creating file at {:?} w/ src {:?}", relative_path, src_path);
                image.create_file(relative_path, src_path.as_os_str())?;
            }

            info!("Committing CIM image");
            image.commit()?;
        }
        CimFSCommands::Fork(args) => {
            // Setup arguments before starting anything
            let from = args.from;
            if from.is_empty() {
                return Err(Error::new(E_INVALIDARG, HSTRING::from("Name was empty")));
            }

            let to = args.to;
            if to.is_empty() {
                return Err(Error::new(E_INVALIDARG, HSTRING::from("Name was empty")));
            }

            trace!("Parsing objects to add");
            // TODO: Add a way to add this from a file schema, oci-manifest, tar, etc.
            let objects = parse_objects_from_args(args.objects)?;

            info!(
                "Creating new CIM at {:?} from {:?}",
                root.join(&to),
                root.join(&from)
            );
            let mut image = Image::new(root, to);

            image.create(Some(from.as_str()))?;

            for o in objects {
                let relative_path = o.get_relative_path()?;
                let src_path = o.get_src_path()?;

                info!("Creating file at {:?} w/ src {:?}", relative_path, src_path);
                image.create_file(relative_path, src_path.as_os_str())?;
            }

            info!("Committing CIM image");
            image.commit()?;
        }
        CimFSCommands::Mount(args) => {
            // Setup arguments before starting anything
            let name = args.image;
            if name.is_empty() {
                return Err(Error::new(E_INVALIDARG, HSTRING::from("Name was empty")));
            }

            info!("Mounting CIM from {:?}", root.join(&name));
            let mut image = Image::new(root, name);

            let mounted_guid = image.mount(args.volume)?;

            let volume_path = format!("\\\\?\\Volume{{{:?}}}", mounted_guid);
            info!("Mounted CIM at {:?}", volume_path);
            println!("{}", volume_path);

            if let Some(mountvol) = args.mountvol {
                image.mount_volume(mountvol.as_str())?;
                info!("Mounted volume at {}\\", mountvol.trim_end_matches('\\'));
            }
        }
        CimFSCommands::Dismount(args) => unsafe {
            let volume = args.volume.as_str().trim_start_matches("Volume{").trim_start_matches("{").trim_end_matches("}");
            let volume = GUID::try_from(volume)
                .map_err(|_| Error::new(E_INVALIDARG, "Invalid GUID".into()))?;
            HRESULT(CimDismountImage(std::ptr::addr_of!(volume) as *const _GUID)).ok()?;
        },
    }

    Ok(())
}

/// Parses a list of object paths into Object structs,
///
fn parse_objects_from_args(list: Vec<String>) -> Result<Vec<Object>> {
    let mut objects = vec![];
    for o in list {
        let mut o = Object::new(o);
        o.resolve_relative_path()?;
        objects.push(o);
    }
    Ok(objects)
}

/// Enable and initialize logging
///
fn enable_logging(trace: bool) {
    if std::env::var("RUST_LOG").ok().is_none() && !trace {
        std::env::set_var("RUST_LOG", "cimutil=info");
    }

    if trace {
        std::env::set_var("RUST_LOG", "cimutil=trace,cimfs=trace,cimfs-sys=trace");
    }

    let sub = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::builder()
                .from_env()
                .expect("should be able to build from env variables"),
        )
        .compact()
        .finish();
    sub.try_init().expect("should init");
}
