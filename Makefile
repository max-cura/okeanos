makefile-path := $(abspath $(lastword $(MAKEFILE_LIST)))
current-dir := $(patsubst %/,%,$(dir $(makefile-path)))
okns-prefix := $(or $(OKNS_ROOT), $(current-dir))

SHELL := /bin/bash

export okns-prefix

build := env "$${params[@]}" $(MAKE) -f $(okns-prefix)/infra/common.mk

.PHONY: lab4-send
lab4-send:
	params+=("b-artefact=lab4-send"); \
	params+=("b-asm-files=boot.S"); \
	$(build)

.PHONY: lab4-recv
lab4-recv:
	params+=("b-artefact=lab4-recv"); \
	params+=("b-asm-files=boot.S stub.S"); \
	$(build)