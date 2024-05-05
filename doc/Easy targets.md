Basically, every one of my `infra/artefacts/*` is doing the exact same thing with slightly different names.

- cargo build
- build assembly files
- link

These things are pretty standardized, as it happens
The only things that *really* change from target to target are the assembly files

Env file
```zsh
export CARGO_INSTALL_ROOT=bin
export OKNS_ROOT=$(pwd)
```

Basically, all I should have to do to create a new device crate is
- select "new cargo crate"
- `crate-type=["staticlib"]`
- `embed build crate-name`

Create `embed` tool:
- `add crate-name`
- `remove crate-name`
- `build crate-name`
