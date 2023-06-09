# v0.1.0-alpha
- Initial binary release build

CLI 

```
Command line utility to work with CimFS on Windows

Usage: cimutil.exe [OPTIONS] <COMMAND>

Commands:
  new       Creates and builds a new CIM image,
  fork      Create and builds a new CIM image based on a pre-existing image,
  mount     Mounts a cim image as a read-only volume,
  dismount  Dismounts a cim image by volume-id,
  help      Print this message or the help of the given subcommand(s)

Options:
      --trace        Enables trace logging
      --root <ROOT>  Sets the root path containing the cim images and data, [default: .]
  -h, --help         Print help (see more with '--help')
```