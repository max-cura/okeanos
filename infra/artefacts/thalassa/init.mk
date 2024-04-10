#
# FILE infra/artefacts/thalassa/init.mk
# DESC Build system initialization for the `thalassa` artefact.
#

include infra/artefacts/thalassa/common.mk

# general target
.PHONY: init
init: init_dirs

# implementation details

init_dirs:
	mkdir -p $(ARTEFACT_BUILD_DIR)

.PHONY: init_dirs