#
# FILE infra/artefacts/theseus-upload/artefact.mk
# DESC The actual build system for the `theseus-upload` artefact.
#

include infra/artefacts/theseus-upload/common.mk

#
# CONFIG
#

.PHONY: _theseus-device_cargo _thalassa_directories

UPLOADER=theseus-upload

#
# FLAGS
#

CARGO_VARS=THESEUS_DEVICE_GIT_DESC="$(shell git rev-parse --short HEAD)" THESEUS_DEVICE_BUILD_DATE="$(shell date -j "+%Y-%m-%d %H:%M:%S %Z")"

#
# INTERNALS
#

GEN_FILES = $(_RUST_TARGET_DIR)/$(UPLOADER) \
			build/theseus-upload/theseus-upload \
			bin/theseus-upload

$(_RUST_TARGET_DIR)/$(UPLOADER): _theseus-device_cargo
	( cd $(CRATE_DIR) ; $(CARGO_VARS) cargo build --profile $(_RUST_PROFILE) )

#
# PUBLIC INTERFACE
#

.PHONY: all rebuild-all rebuild-cargo rebuild-infra clean-all clean-cargo clean-infra

build/theseus-upload/$(UPLOADER): $(_RUST_TARGET_DIR)/$(UPLOADER)
	cp $^ $@

bin/$(UPLOADER): build/theseus-upload/$(UPLOADER)
	cp $^ $@

all: bin/theseus-upload

rebuild-all: clean-all all
rebuild-cargo: clean-cargo all
rebuild-infra: clean-infra all

clean-all: clean-cargo clean-infra
clean-cargo:
	( cd $(CRATE_DIR) ; cargo clean )
clean-infra:
	rm -f $(GEN_FILES)