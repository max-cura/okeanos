#
# FILE infra/artefacts/theseus-device/init.mk
# DESC Build system initialization for the `theseus-device` artefact.
#

include infra/artefacts/theseus-upload/common.mk

# general target
.PHONY: init
init: init_dirs

# implementation details

init_dirs:
	mkdir -p $(ARTEFACT_BUILD_DIR)
	mkdir -p bin

.PHONY: init_dirs