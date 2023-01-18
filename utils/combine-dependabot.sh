#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
CUBIST_DIR="$SCRIPT_DIR/.."

function combine() {
    local kind="${1:-}"
    local package_file=""
    case "$kind" in
        "npm")
            package_file="$CUBIST_DIR/cubist-node-sdk/package.json"
            ;;
        "cargo")
            package_file="$CUBIST_DIR/Cargo.toml"
            ;;
        *)
            echo "ERROR: Unknown kind '${kind}'.  Kind must be either 'npm' or 'cargo'"
            return 1;
            ;;
    esac

    local patches_dir="patches"    
    mkdir -p "${patches_dir}"
    cd "${patches_dir}"
    git branch -r | grep origin/dependabot/$kind | while read -r branch; do
        file=`git format-patch --minimal -1 $branch $package_file`
        if git am $file ; then
            echo "Applied $file"
            rm $file
        else
            echo "Failed to apply $file"
            git am --abort
        fi
    done
    echo "Apply remaining patches from ${patches_dir} directory"
}

combine "$@"

