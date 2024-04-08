Boilerplate and metadata, along with the VERY IMPORTANT `llvm-target`.
```json
{
    "llvm-target": "armv6zk-none-eabihf",
    "metadata": {
        "description": "For arm1176jzf-s",
        "host_tools": null,
        "std": null,
        "tier": null
    },
    "is-builtin": false,
```
arm1176jzf-s has VFPv2 floating point coprocessor, which means we use the eabihf ABI.
We do not have NEON though.
```json
    "abi": "eabihf",
    "arch": "arm",
    "target-pointer-width": "32",
    "target-c-int-width": "32",
    "features": "+v6,+vfp2,-d32,-neon,+strict-align",
```
This is the default data layout taken from LLVM's armv6z-none-eabihf target
```json
    "data-layout": "e-m:e-p:32:32-Fi8-i64:64-v128:64:128-a:0:32-n32-S64",
```
TBH I'm not 100% sure whether this is allowed but I'm pretty sure it's correct
```json
    "c-enum-min-bits": 8,
```
Linker stuff:
```json
    "linker-flavor": "gcc",
    "linker": "arm-none-eabi-gcc",
```
If we were building as an executable, we would want:
```json
    "pre-link-args": {
        "gcc": [ "-Wall", "-nostdlib", "-nostartfiles", "-ffreestanding", "-march=armv6" ]
    },
    "executables": true,
    "relocation-model": "static",
```
Various other stuff;
- panic should abort
- no redzones, we're in the kernel
```json
    "panic-strategy": "abort",
    "disable-redzone": true,
```
ARMv6 doesn't have hardware CAS, just LDREX/STREX/SWP/SWPB.
Truthfully, I'm unsure about max-atomic-width--it's possible that this should be 32, but I also saw
64 on armv6-unknown-freebsd so I'm not really sure what's going on.
```json
    "max-atomic-width": 64,
    "atomic-cas": false,
```
Don't know what this does:
```json
    "crt-objects-fallback": "false",
    "emit-debug-gdb-scripts": false,
    "asm-args": [
        "-march=armv6z",
        "-mlittle-endian"
    ]
```
The end:
```json
}
```