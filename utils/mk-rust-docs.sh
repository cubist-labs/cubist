#!/bin/bash

set -eu

SCRIPT_DIR="$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
CUBIST_DIR="$SCRIPT_DIR/.."

cd "${CUBIST_DIR}"
env RUSTDOCFLAGS='-D warnings --html-in-header utils/rustdoc-syntax-highlighting/in-header.html --html-after-content utils/rustdoc-syntax-highlighting/after-content.html' cargo doc --package 'cubist-*' --no-deps ${CARGO_DOC_EXTRA_ARGS}
