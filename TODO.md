- [ ] add `prune` flag and implement checking for orphaned files
- [ ] more comprehensive `dry_run` behavior
- [ ] make jinja macro to generate palette table markdown (with a column for the
      roles assigned to each swatch)
- [ ] check for scheme name collisions
- [ ] put extra settings back into the kitty template?
- [ ] add json schema for schemes?
- [ ] streamline macro invocations?

# errors

- [ ] fix unknown variables in templates returning `InternalBug` (might already
      be fixed?)
  - [ ] have it show the unknown variable
- [x] investigate whether it's a good idea that both `Error` and `RenderError`
      convert from `SchemeError` and `TemplateError`
- [ ] collect non-fatal errors and then return instead of exiting immediately
  - [ ] group errors together to lessen unnecessary verbosity
- [ ] print more logs by default
- [ ] if an undefined role is used as a value in a scheme, ask the user if they
      meant to prefix it with a `$` in the error message

# schemes

- [ ] move `scheme` (and the upstream stuff) under `meta`
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

- [ ] make `creamsicle` slightly yellower (or `sand` slightly redder?)
- [ ] add four additional colors to the rainbow brackets for a total of 10
      levels
