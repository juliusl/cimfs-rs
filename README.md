# CimFS for Rust 

The Composite Image File system is a file system in Windows based on a flat file structure. A detailed explanation of this file system can be found here: https://learn.microsoft.com/en-us/windows/win32/api/_cimfs/.

This repo provides bindings, wrappers, and a utility binary to facilitate working with CimFS.

- **cimfs-sys**: Is a rust-bindgen library for `CimFs.h` linking to `cimfs.lib` in Windows.
- **cimfs**: Is a library that provides a wrapper api over the functions generated in `cimfs-sys` and is built on top of `windows-rs` primitives.
- **cimutil**: Is a binary produced by the `cimfs` repo and demonstrates how to consume the library. The binary also provides a basic end-to-end cli for working with CimFS on Windows.

## Getting Started w/ cimfs library

There are two main types that this library provides, `Image` and `Object`. 

An example usage would look like the following,

```rs
// Creates a new cim image
let image = Image::new("c:\\cim", "image.cim");

image.create(None)?;

image.create_file("Cargo.toml", ".\\Cargo.toml")?;

image.commit()?;

// Creates a fork of the above image
let image = Image::new("c:\\cim", "image01.cim");

image.create(Some("image.cim"))?;

image.create_file(".gitignore", ".\\.gitignore")?;

image.commit()?;

//
// The image handle will be closed when `image` goes out of scope
//
```

A more advanced example would use the `Object` struct, which provides utilities for generating the parameters for `create_file`, to add multiple files at once.

**Note**: When creating new files in a CIM image, ancestors are not automatically added because the file attributes cannot be inferred. This is the gap that `Object` is filling.

```rs
// Create a fork of the above and add multiple files at once
let mut objects = vec![];
// The ordered set ensures that ancestors are added in the correct order
// If this does not happen, creating the file will fail
let mut ancestors = BTreeSet::new();
for o in list {
    // `o` can be a file path or directory path
    let mut o = Object::new(o);
    // This function will output the ancestors required to add this object to the image
    let mut a = o.resolve_relative_path(true)?;
    // Keep the above set updated
    ancestors.append(&mut a);
    objects.push(o);
}

let mut image = Image::new("c:\\cim", "image03.cim");
image.create(Some("image.02.cim"))?;

// Consume the above collections to call the `build()` function
image.build(objects, ancestors)?;

image.commit()?;
```

## Example CLI Usage

In addition to the library, this repo also provides a binary to work directly with CimFS.

**Note** You can use `--help` argument with any command to view documentation.

Here is a basic example that creates a new Cim image,

```ps
cimutil.exe --root .cimroot new --name image.cim Cargo.toml Cargo.lock .gitignore cimfs\src\lib.rs cimfs\src\image.rs
```

In addition you can also use this utility to mount the filesystem. Note that creating and forking images does not require elevated permissions, however mounting a Cim does require elevated permissions

**Caveat** CimFS can only be mounted as read-only.

```ps
# Requires Elevated Permissions
cimutil.exe --root .cimroot mount image.cim
```

This will output a volume path that will look something like this: `\\?\Volume{93B0CD56-86B0-43FA-820E-2E421CBE7411}`. 

Once mounted you should be able to see the volume listed in the output of the `mountvol` command. You can also use command to assign a drive letter like so,

```ps
mountvol G: '\\?\Volume{93B0CD56-86B0-43FA-820E-2E421CBE7411}'
```

Or, combine both commands,

```ps
mountvol G: $(cimutil.exe --root .cimroot mount image.cim)
```

In addition, the `mount` command also includes a `--mountvol` flag that will mount the volume after the file system is mounted. 

This shortens the above into the following,

```ps
cimutil.exe --root .cimroot mount --mountvol 'G:' image.cim
```

Lastly, to dismount the image you can use `dismount` like so, 

```ps
# All of the following are equivalent
cimutil.exe dismount '\\?\Volume{93B0CD56-86B0-43FA-820E-2E421CBE7411}'
cimutil.exe dismount 'Volume{93B0CD56-86B0-43FA-820E-2E421CBE7411}'
cimutil.exe dismount '{93B0CD56-86B0-43FA-820E-2E421CBE7411}'
cimutil.exe dismount '93B0CD56-86B0-43FA-820E-2E421CBE7411'
```

**Tip** The `Winobj.exe` application from `https://live.sysinternals.com/` can be used to inspect the mounted volumes and even the created mount points.

Once mounted to a mountpoint, the volume should then be available in Explorer, cli, etc.

## Limitations

The following api's do not have direct support by `cimfs::api::`, although the bindings do exist in either `cimfs_sys::` or `cimfs::raw::`.

**Note** It is recommended to use `windows-rs` types when possible, even though `cimfs_sys` may provide duplicated types. This is a side-effect of using bindgen to generate the bindings for `CimFs.h`.

- `CimCreateAlternateStream`
- `CimCreateHardLink`
- `CimDeletePath`

In addition, when creating files in a cim image, the extended attribute's buffer and security descriptor buffer are not currently being used.
