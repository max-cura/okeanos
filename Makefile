makefile-path := $(abspath $(lastword $(MAKEFILE_LIST)))
current-dir := $(patsubst %/,%,$(dir $(makefile-path)))
okns-prefix := $(or $(OKNS_ROOT), $(current-dir))

SHELL := /bin/bash

export okns-prefix

# PARAMETERS
#  	p-artefact		name of generated library
#	p-asm-files		names of assembly files in $PKG/extern
# 	p-asm-root
#	p-package		name of cargo package
#	p-linker-script
#	p-cargo-profile

build := env "$${params[@]}" $(MAKE) -f $(okns-prefix)/infra/common.mk

.PHONY: okboot
okboot:
	params+=("p-artefact=okboot"); \
	params+=("p-asm-files=boot.S stub.S"); \
	$(build)
