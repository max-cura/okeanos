# parameters:
# 	p-artefact
#	p-asm-files
#	p-asm-root 		(default: "extern")
#	p-package		(default: p-artefact)
#	p-linker-script (default: def-linker-script)
# 	p-cargo-profile (default: "release")
#
# If specified, p-asm-root and p-linker-script are relative to the package root
# p-asm-files are relative to p-asm-root

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

dep-flags = -MT $@ -MMD -MP -MF $(z-asm-dep-dir)/$*.d

ld-flags := $(freestanding-flags) $(tune-flags)
ld-flags += -Wl,--gc-sections
ld-flags += -z noexecstack

# target setup

z-artefact := $(p-artefact)

p-package ?= $(p-artefact)
z-package := $(p-package)
z-package-root := $(okns-src-device)/$(z-package)
z-build-root := $(okns-build-prefix)/$(z-package)

p-asm-root ?= extern
z-asm-root := $(z-package-root)/$(p-asm-root)
z-asm-files := $(p-asm-files)

p-cargo-profile ?= release
z-cargo-profile := $(p-cargo-profile)
z-cargo-target-profile := $(p-cargo-profile)
ifeq ($(z-cargo-profile), $(filter $(z-cargo-profile), dev test))
	z-cargo-target-profile := debug
endif
ifeq ($(z-cargo-profile), bench)
	z-cargo-target-profile := release
endif
z-cargo-target-dir := $(cargo-target-prefix)/$(target)/$(z-cargo-target-profile)

ifdef p-linker-script
	p-linker-script := $(addprefix $(z-package-root)/,$(p-linker-script))
endif
p-linker-script ?= $(def-linker-script)
z-linker-script := $(p-linker-script)

# generated files

z-lib-file := $(z-cargo-target-dir)/lib$(z-artefact).a
z-elf-file := $(z-build-root)/$(z-artefact).elf
z-list-file := $(z-build-root)/$(z-artefact).list
z-bin-file := $(z-build-root)/$(z-artefact).bin

z-asm-obj-files := $(z-asm-files:%.S=$(z-build-root)/%.o)
z-asm-dep-dir := $(z-build-root)/deps
z-asm-dep-files := $(z-asm-files:%.S=$(z-asm-dep-dir)/%.d)

z-gen-files := $(z-lib-file) $(z-elf-file) $(z-list-file) $(z-bin-file) \
			   $(z-asm-obj-files) $(z-asm-dep-files)

.PHONY: clean all

all: $(z-bin-file) $(z-list-file)

clean:
	cargo clean -p $(z-package)
	rm -f $(z-gen-files)

.PHONY: phony-cargo

# When invoking cargo from workspace root, it won't load the config.toml properly
# [1]: https://doc.rust-lang.org/cargo/reference/config.html
$(z-lib-file): phony-cargo
	( cd $(okns-src-device); cargo build --profile $(z-cargo-profile) -p $(z-package) )

$(z-asm-dep-dir): ; @mkdir -p $@
$(z-build-root): ; @mkdir -p $@

$(z-build-root)/%.o: $(z-asm-root)/%.S $(z-asm-dep-dir)/%.d | $(z-asm-dep-dir)
	$(arm-none-eabi-gcc) $(as-flags) -c -o $@ $<

$(z-elf-file): $(z-asm-obj-files) $(z-lib-file) | $(z-build-root)
	$(arm-none-eabi-gcc) -T $(z-linker-script) $(ld-flags) $^ -o $@

$(z-list-file): $(z-elf-file) | $(z-build-root)
	$(arm-none-eabi-objdump) -D $< > $@

$(z-bin-file): $(z-elf-file) | $(z-build-root)
	$(arm-none-eabi-objcopy) $< -O binary $@

$(z-asm-dep-files):

include $(wildcard $(z-asm-dep-files))