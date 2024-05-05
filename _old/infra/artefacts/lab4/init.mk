#
# FILE infra/artefacts/lab4-send/init.mk
# DESC Build system initialization for the `theseus-device` artefact.
#

include infra/artefacts/lab4/common.mk

# general target
.PHONY: init
init: init_dirs

# implementation details

init_dirs:
	mkdir -p $(ARTEFACT_BUILD_DIR)

.PHONY: init_dirs