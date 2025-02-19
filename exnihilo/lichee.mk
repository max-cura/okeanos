.SUFFIXES:

okns-build-prefix := $(okns-prefix)/build
okns-bin-prefix := $(okns-prefix)/bin
okns-infra-prefix := $(okns-prefix)/infra

cargo-target-prefix := $(okns-prefix)/target

arm-gnu-prefix := /applications/armgnutoolchain/13.2.Rel1/arm-none-eabi
arm-none-eabi-gcc := $(arm-gnu-prefix)/bin/arm-none-eabi-gcc
arm-none-eabi-objdump := $(arm-gnu-prefix)/bin/arm-none-eabi-objdump
arm-none-eabi-objcopy := $(arm-gnu-prefix)/bin/arm-none-eabi-objcopy

def-linker-script := $(okns-infra-prefix)/linker/default.ld

# target setup

build-artefact := lichee

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
cargo-target-dir := $(cargo-target-prefix)/riscv64gc-unknown-none-elf/$(cargo-target-profile)

# files

lib-file := $(cargo-target-dir)/lib$(artefact).a
elf-file := $(build-root)/$(artefact).elf
list-file := $(build-root)/$(artefact).list

gen-files := $(lib-file) $(elf-file) $(list-file) $(bin-file)

#

.PHONY: clean all

all: $(bin-file) $(list-file)

clean:
	cargo clean -p lichee
	rm -f $(gen-files)

.PHONY: phony-cargo

$(lib-file): phony-cargo
	( cd exnihilo ; cargo build --profile release -p lichee )

$(asm-dep-dir): ; @mkdir -p $@
$(build-root): ; @mkdir -p $@

$(elf-file): $(lib-file) | $(build-root)
	riscv64-unknown-elf-ld -T exnihilo/lichee.ld $(ld-flags) $^ -o $@

$(list-file): $(elf-file) | $(build-root)
	riscv64-unknown-elf-objdump -D $< > $@

$(bin-file): $(elf-file) | $(build-root)
	riscv64-unknown-elf-objcopy $< -O binary $@

# include $(wildcard $(dep-files))