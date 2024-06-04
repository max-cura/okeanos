makefile-path := $(abspath $(lastword $(MAKEFILE_LIST)))
current-dir := $(patsubst %/,%,$(dir $(makefile-path)))
okns-prefix := $(or $(OKNS_ROOT), $(current-dir))

SHELL := /bin/bash

export okns-prefix

build := env "$${params[@]}" $(MAKE) -f $(okns-prefix)/infra/common.mk

.PHONY: lab2
lab2:
	params+=("b-artefact=lab2"); \
	params+=("b-asm-files=boot.S"); \
	$(build)

.PHONY: lab4-send
lab4-send:
	params+=("b-artefact=lab4_send"); \
	params+=("b-package=lab4-send"); \
	params+=("b-asm-files=boot.S"); \
	$(build)

.PHONY: lab4-recv
lab4-recv:
	params+=("b-artefact=lab4_recv"); \
	params+=("b-package=lab4-recv"); \
	params+=("b-asm-files=boot.S stub.S"); \
	$(build)

.PHONY: lab7-wave
lab7-wave:
	params+=("b-artefact=lab7_wave"); \
	params+=("b-package=lab7-wave"); \
	params+=("b-linker-script=thalassa.ld"); \
	params+=("b-asm-files=boot.S"); \
	$(build)

.PHONY: lab7-scope
lab7-scope:
	params+=("b-artefact=lab7_scope"); \
	params+=("b-package=lab7-scope"); \
	params+=("b-linker-script=thalassa.ld"); \
	params+=("b-asm-files=boot.S"); \
	$(build)

.PHONY: bismuth
bismuth:
	params+=("b-artefact=bismuth"); \
	params+=("b-asm-files=boot.S lic.S thread.S"); \
	$(build)

.PHONY: dmr-passthru dmr-server dmr-client
dmr-passthru:
	params+=("b-artefact=dmr_passthru"); \
  	params+=("b-package=dmr-passthru"); \
	params+=("b-asm-files=boot.S lic.S thread.S"); \
  	$(build)

dmr-client:
	params+=("b-artefact=dmr_client"); \
  	params+=("b-package=dmr-client"); \
	params+=("b-asm-files=boot.S lic.S thread.S"); \
  	$(build)

dmr-server:
	params+=("b-artefact=dmr_server"); \
  	params+=("b-package=dmr-server"); \
	params+=("b-asm-files=boot.S lic.S thread.S"); \
  	$(build)

.PHONY: theseus-device
theseus-device:
	params+=("b-artefact=theseus_device"); \
	params+=("b-package=theseus-device"); \
	params+=("b-asm-files=boot.S stub.S"); \
	$(build)

.PHONY: theseus-upload
theseus-upload:
	cargo build -p theseus-upload --release

.PHONY: host-install
host-install: theseus-upload
	cp $(okns-prefix)/target/release/theseus-upload $(okns-prefix)/bin