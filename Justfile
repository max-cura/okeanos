#! /usr/bin/env just -- justfile

default:
    @just `just _get_last frontend-command "help" | tr -d '"'`

# actual frontend targets

build *TARGET:
    @just _dispatch build {{ TARGET }}

clean *TARGET:
    @just _dispatch clean {{ TARGET }}

run TARGET *ARG:
    @just _dispatch-one run {{ TARGET }} {{ ARG }}

antimony:
    @just _dispatch-one build antimony
    xfel ddr d1
    xfel write 0x40000000 build/antimony/antimony.bin
    xfel exec 0x40000000

help:
    #!/usr/bin/env nu
    just --list
    print "Targets:"
    let list = ['antimony-setup' 'bismuth-setup']
    (just --dump --unstable --dump-format json | from json).recipes
        | transpose recipe data
        | flatten
        | where {|row| $row.recipe in $list}
        | select recipe doc
        | each {|row| $"\t($row.recipe | sed 's/-.*//')\t($row.doc)" }
        | str join "\n"
        | $in + "\n"
        | column -t -s"\t"
        | sed 's/^/\t/'

# target definitions

[no-exit-message]
[private]
default-setup NEXT:
    #!/usr/bin/env bash
    export last_modified_device_dir=$(find device -type d \( -depth 1 \) -print0 \
      | xargs -0 gstat --format '%Y :%y %n' \
      | sort -nr | cut -d: -f2- | cut -d' ' -f4- \
      | head -n1 \
      | cut -d/ -f2-)
    just _dispatch ${last_modified_device_dir} ${_J_RECIPE}

# Antimony experimental system for Allwinner Nezha D1
[no-exit-message]
[private]
antimony-setup NEXT:
    #!/usr/bin/env bash
    export J_TRIPLE=riscv64gc-unknown-none-elf
    export J_BINUTILS_PREFIX=riscv64-unknown-elf
    export J_LINKER_OPTS='-z noexecstack -nostdlib -Wl,--gc-sections -nostdlib -ffreestanding -nostartfiles -fPIC'
    {{ NEXT }}

# Bismuth experimental system for BCM2835
[no-exit-message]
[private]
bismuth-setup NEXT:
    #!/usr/bin/env bash
    export J_TRIPLE=armv6zk-none-eabihf
    export J_BINUTILS_PREFIX=arm-none-eabi
    export J_LINKER_OPTS='-z noexecstack -Wl,--gc-sections -nostdlib -ffreestanding -nostartfiles \
                          -mcpu=arm1176jzf-s -march=armv6zk+fp -mfpu=vfpv2 -mfloat-abi=hard -fPIC'
    {{ NEXT }}

[no-exit-message]
[private]
okboot-setup NEXT:
    #!/usr/bin/env bash
    export J_TRIPLE=armv6zk-none-eabihf
    export J_BINUTILS_PREFIX=arm-none-eabi
    export J_LINKER_OPTS='-z noexecstack -Wl,--gc-sections -nostdlib -ffreestanding -nostartfiles \
                          -mcpu=arm1176jzf-s -march=armv6zk+fp -mfpu=vfpv2 -mfloat-abi=hard -fPIC \
                          build/okdude/{boot,elf,stub}.o'
    just _build-dir okdude
    for file in "boot" "elf" "stub" ; do
        arm-none-eabi-gcc -nostdlib -ffreestanding -nostartfiles -mcpu=arm1176jzf-s -march=armv6zk+fp -mfpu=vfpv2 \
          -mfloat-abi=hard -fPIC -Wa,--warn -Wa,--fatal-warnings -c device/okboot/extern/$file.S -o build/okdude/$file.o
    done
    {{ NEXT }}

# target setup dispatcher

[no-exit-message]
_dispatch RECIPE *TARGETS:
    @if [[ "{{ TARGETS }}" = "" ]] ; \
      then just _dispatch-one "{{ RECIPE }}" `just _get_last target` ; \
      else for target in {{ TARGETS }}; do just _dispatch-one "{{ RECIPE }}" $target; done ; fi

[no-exit-message]
_dispatch-one RECIPE TARGET *EXTRA:
    @just _remember target {{ TARGET }}
    @just _remember frontend-command {{ RECIPE }}
    @_J_RECIPE={{ RECIPE }} just {{ TARGET }}-setup "just _task-{{ RECIPE }} {{ TARGET }} {{ EXTRA }}"

# task muxers

[no-exit-message]
_task-clean TARGET:
    @just _clean-single {{ TARGET }}

[no-exit-message]
_task-build TARGET: (_build-dir TARGET)
    @just _build-cargo {{ TARGET }}
    @just _link {{ TARGET }}
    @just _dump {{ TARGET }}
    @just _copy {{ TARGET }}

[no-exit-message]
_task-run TARGET *EXTRA: (_task-build TARGET)
    @just _run {{ TARGET }} {{ EXTRA }}

# implementations

[no-exit-message]
_build-cargo DEVICE_PACKAGE PROFILE="release":
    ( cd device/{{ DEVICE_PACKAGE }} ; cargo build --profile {{ PROFILE }} -p {{ DEVICE_PACKAGE }} )

[no-exit-message]
_link DEVICE_PACKAGE PROFILE="release" LIBRARY=DEVICE_PACKAGE:
    @just _cmd_proxy "${J_BINUTILS_PREFIX}-gcc \
        -T device/{{ DEVICE_PACKAGE }}/${J_LINKER_SCRIPT:-${J_TRIPLE}.ld} \
        ${J_LINKER_OPTS} target/${J_TRIPLE}/{{ PROFILE }}/lib{{ LIBRARY }}.a \
        -o build/{{ DEVICE_PACKAGE }}/{{ DEVICE_PACKAGE }}.elf"

[no-exit-message]
_dump DEVICE_PACKAGE:
    @just _cmd_proxy "${J_BINUTILS_PREFIX}-objdump \
        -D build/{{ DEVICE_PACKAGE }}/{{ DEVICE_PACKAGE }}.elf \
        > build/{{ DEVICE_PACKAGE }}/{{ DEVICE_PACKAGE }}.s"

[no-exit-message]
_copy DEVICE_PACKAGE:
    @just _cmd_proxy "${J_BINUTILS_PREFIX}-objcopy \
        build/{{ DEVICE_PACKAGE }}/{{ DEVICE_PACKAGE }}.elf \
        -O binary build/{{ DEVICE_PACKAGE }}/{{ DEVICE_PACKAGE }}.bin"

[no-exit-message]
_cmd_proxy CMD:
    {{ CMD }}

[no-exit-message]
_clean-single DEVICE_PACKAGE:
    rm -rf build/{{ DEVICE_PACKAGE }}/*
    cargo clean -p {{ DEVICE_PACKAGE }}

_build-dir TARGET="":
    @if ! [[ -d build/{{ TARGET }} ]] ; then just _cmd_proxy "mkdir -p build/{{ TARGET }}" ; fi

[no-exit-message]
_run TARGET *ARGS:
    okdude build/{{ TARGET }}/{{ TARGET }}.elf {{ ARGS }}

# command memory

_get_last KEY DEFAULT="default":
    @if ! [[ -f "${XDG_CACHE_HOME}/okeanos-justfile/history.json" ]] ; \
      then echo "{{ DEFAULT }}" ; \
      else cat ${XDG_CACHE_HOME}/okeanos-justfile/history.json \
            | jq -c 'if has("{{ KEY }}") then ."{{ KEY }}" else "{{ DEFAULT }}" end' ; \
      fi

_remember RECIPE TARGET:
    @if [[ "{{ TARGET }}" != default ]] ; then \
        just _set_last "{{ RECIPE }}" "{{ TARGET }}"; \
    fi

_set_last RECIPE ARGS:
    @if ! [[ -f "${XDG_CACHE_HOME}/okeanos-justfile/history.json" ]] ; then mkdir -p "${XDG_CACHE_HOME}/okeanos-justfile/" ; echo "{}" > ${XDG_CACHE_HOME}/okeanos-justfile/history.json ; fi
    @cat ${XDG_CACHE_HOME}/okeanos-justfile/history.json \
      | jq -c '. + {"{{ RECIPE }}":"{{ ARGS }}"}' \
      | sponge ${XDG_CACHE_HOME}/okeanos-justfile/history.json
