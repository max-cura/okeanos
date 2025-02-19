makefile-path := $(abspath $(lastword $(MAKEFILE_LIST)))
current-dir := $(patsubst %/,%,$(dir $(makefile-path)))
okns-prefix := $(or $(OKNS_ROOT), $(current-dir))

SHELL := /bin/bash

export okns-prefix

# PARAMETERS
#  	build-artefact		name of generated library
#	build-asm-files		names of assembly files in $PKG/extern
# 	build-asm-root
#	build-package		name of cargo package
#	build-linker-script
#	build-cargo-profile

build := env "$${params[@]}" $(MAKE) -f $(okns-prefix)/infra/common.mk

.PHONY: okboot bismuth
okboot:
	params+=("build-artefact=okboot"); \
	params+=("build-asm-files=boot.S stub.S elf.S"); \
	$(build)

bismuth:
	params+=("build-artefact=bismuth"); \
	params+=("build-asm-files=boot.S int.S thread.S"); \
	$(build)

lichee:
	$(MAKE) -f $(okns-prefix)/exnihilo/lichee.mk