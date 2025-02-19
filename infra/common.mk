# parameters:
# 	build-artefact
#	build-asm-files
#	build-asm-root 		(default: "extern")
#	build-package		(default: build-artefact)
#	build-linker-script (default: def-linker-script)
# 	build-cargo-profile (default: "release")
#
# If specified, build-asm-root and build-linker-script are relative to the package root
# build-asm-files are relative to build-asm-root

.SUFFIXES:

# definitions

okns-build-prefix := $(okns-prefix)/build
okns-bin-prefix := $(okns-prefix)/bin
okns-infra-prefix := $(okns-prefix)/infra

cargo-target-prefix := $(okns-prefix)/target

okns-infra-script-dir := $(okns-infra-prefix)/scripts
okns-infra-target-dir := $(okns-infra-prefix)/targets

okns-src-device := $(okns-prefix)/device
okns-src-host := $(okns-prefix)/host

arm-gnu-prefix := /applications/armgnutoolchain/13.2.Rel1/arm-none-eabi
arm-none-eabi-gcc := $(arm-gnu-prefix)/bin/arm-none-eabi-gcc
arm-none-eabi-objdump := $(arm-gnu-prefix)/bin/arm-none-eabi-objdump
arm-none-eabi-objcopy := $(arm-gnu-prefix)/bin/arm-none-eabi-objcopy

def-linker-script := $(okns-infra-prefix)/linker/default.ld

# setup

cpu := arm1176jzf-s
target := armv6zk-none-eabihf

tune-flags := -mcpu=$(cpu) -march=armv6zk+fp -mfpu=vfpv2 -mfloat-abi=hard

# flags

freestanding-flags := -nostdlib -ffreestanding -nostartfiles

as-flags := $(freestanding-flags) $(tune-flags:%=-Wa,%) -fPIC
as-flags += -Wa,--warn -Wa,--fatal-warnings
as-flags += -I$(okns-src-device)/extern

dep-flags = -MT $@ -MMD -MP -MF $(asm-dep-dir)/$*.d

ld-flags := $(freestanding-flags) $(tune-flags)
ld-flags += -Wl,--gc-sections
ld-flags += -z noexecstack

# target setup

artefact := $(build-artefact)

build-package ?= $(build-artefact)
package := $(build-package)
package-root := $(okns-src-device)/$(package)
build-root := $(okns-build-prefix)/$(package)

build-asm-root ?= extern
asm-root := $(package-root)/$(build-asm-root)
asm-files := $(build-asm-files)

build-cargo-profile ?= release
cargo-profile := $(build-cargo-profile)
cargo-target-profile := $(build-cargo-profile)
ifeq ($(cargo-profile), $(filter $(cargo-profile), dev test))
	cargo-target-profile := debug
endif
ifeq ($(cargo-profile), bench)
	cargo-target-profile := release
endif
cargo-target-dir := $(cargo-target-prefix)/$(target)/$(cargo-target-profile)

ifdef build-linker-script
	build-linker-script := $(addprefix $(package-root)/,$(build-linker-script))
endif
build-linker-script ?= $(def-linker-script)
linker-script := $(build-linker-script)

# generated files

lib-file := $(cargo-target-dir)/lib$(artefact).a
elf-file := $(build-root)/$(artefact).elf
list-file := $(build-root)/$(artefact).list
bin-file := $(build-root)/$(artefact).bin

asm-obj-files := $(asm-files:%.S=$(build-root)/%.o)
asm-dep-dir := $(build-root)/deps
asm-dep-files := $(asm-files:%.S=$(asm-dep-dir)/%.d)

gen-files := $(lib-file) $(elf-file) $(list-file) $(bin-file) \
			   $(asm-obj-files) $(asm-dep-files)

.PHONY: clean all

all: $(bin-file) $(list-file)

clean:
	cargo clean -p $(package)
	rm -f $(gen-files)

.PHONY: phony-cargo

# When invoking cargo from workspace root, it won't load the config.toml properly
# [1]: https://doc.rust-lang.org/cargo/reference/config.html
$(lib-file): phony-cargo
	( cd $(okns-src-device); cargo build --profile $(cargo-profile) -p $(package) )

$(asm-dep-dir): ; @mkdir -p $@
$(build-root): ; @mkdir -p $@

$(build-root)/%.o: $(asm-root)/%.S $(asm-dep-dir)/%.d | $(asm-dep-dir)
	$(arm-none-eabi-gcc) $(as-flags) -c -o $@ $<

$(elf-file): $(asm-obj-files) $(lib-file) | $(build-root)
	$(arm-none-eabi-gcc) -T $(linker-script) $(ld-flags) $^ -o $@

$(list-file): $(elf-file) | $(build-root)
	$(arm-none-eabi-objdump) -D $< > $@

$(bin-file): $(elf-file) | $(build-root)
	$(arm-none-eabi-objcopy) $< -O binary $@

$(asm-dep-files):

include $(wildcard $(asm-dep-files))