- [ ] fix `#:tombi format.disabled = true` being added twice
- [ ] fix `failed to discover git repo from path`
- [ ] add `prune` flag and implement checking for orphaned files
- [ ] more comprehensive `dry_run` behavior
- [ ] make jinja macro to generate palette table markdown (with a column for the
      roles assigned to each swatch)
- [ ] check for scheme name collisions
- [ ] put extra settings back into the kitty template?
- [ ] add json schema for schemes?
- [ ] use strum in `render::objects::Color`?
- [ ] normalize file extensions
- [ ] organzize items in files (`pub`lic and more general items first)
- [ ] use consistent order for organizing `use` and `mod` statements:
      `use core::{...}` -> `use std::{...}` -> `use ...` -> `use crate::{...}`
      -> `mod` -> `use self::{...}` -> `pub mod ...` / `pub(crate) mod ...` ->
      `pub use::{...}` / `pub(crate) use::{...}`
- [ ] make sure basel uses the workspace / repo / `basel.toml` root
- [ ] invalidate manifest cache on config changes?
- [x] nest basel modules, there are getting to be too many for a flat structure
  - [x] increase encapsulation between (sub)modules

# errors

- [ ] ~~consolidate submodule errors into module error type~~ probably not
      actually
- [ ] fix unknown variables in templates returning `InternalBug` (might already
      be fixed?)
  - [ ] have it show the unknown variable
- [ ] collect non-fatal errors and then return instead of exiting immediately
  - [ ] group errors together to lessen unnecessary verbosity
- [ ] print more logs by default
- [ ] if an undefined role is used as a value in a scheme, ask the user if they
      meant to prefix it with a `$` in the error message
- [x] ~~investigate whether it's a good idea that both `Error` and `RenderError`
      convert from `SchemeError` and `TemplateError`~~ refactored to use anyhow
      for these situations
- [x] ~~streamline macro invocations?~~ removed error macros entirely

# schemes

- [ ] move `scheme` (and the upstream stuff) under `meta`?
  - [ ] probably not actually
- [ ] make `meta`'s ascii versions defined in the same way as swatches, i.e.
      `[meta]
      scheme = "cutiepro"` for defining the display name only and
      accepting automatic ascii fallbacks if necessary, and
      `[meta]
      scheme = { name = "cutiepró", ascii = "cutiedumb" }` for
      explicitly setting the ascii version
- [ ] add option to manually set / override the main upstream repo (project
      root)
  - [ ] and some way to set / override `upstream_repo` and `upstream_file` in
        templates.
    - [ ] also maybe `upstream_repo` as it's currently defined isn't ever
          actually useful?
- [x] give `scheme` an ascii fallback too. (also check it for length?)
- [x] check `Meta` fields for length

# palettes

- [ ] add oklch support
  - [ ] replace hex_color with palette
  - [ ] add support for other formats like hsl and hsv?
  - [ ] add option to normalize palette to a single color space / format

# roles

- [ ] allow direct hex colors as role values in schemes and make the palette
      table technically optional?
- [ ] rethink role design around headings and rainbow punctuation
  - [ ] make `rainbow` an optional array under a new `features` table

# personal

- [ ] show current directory and running command in fish title
- [ ] fix my custom Helix move lines bindings not working right with multi-line
      selections

## cutiepro

- [ ] add four additional colors to the rainbow brackets for a total of 10
      levels
- [x] make `creamsicle` slightly yellower (or `sand` slightly redder?)
