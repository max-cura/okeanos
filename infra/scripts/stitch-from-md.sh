#!/bin/sh
# Copyright © 2024 Maximilien M. Cura. All rights reserved.
#
# proj okeanos
# file infra/scripts/stitch-from-md.sh
# desc Print out the contents of all mutliline code blocks in a markdown file.
#      Used to generate some annotated configuration files (e.g. JSON files) that don't allow
#      comments for explaining why different choices were made.

if [ "$#" -ne 2 ]; then
    echo "usage: $0 <md-file> <delim>" >&2
    exit 1
elif ! [ -e $1 ]; then
    echo "$1 not found" >&2
    exit 1
elif ! [ -f $1 ]; then
    echo "$1 is not a file" >&2
fi

awk '/```'"$2"'/{flag=1;next}/```/{flag=0}flag' $1