#!/usr/bin/env python3
# Copyright (c) 2025 vivo Mobile Communication Co., Ltd.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
"""Compiler driver of the `kmiri_riscv64` toolchain.

The toolchain runs in GN's Miri mode: `run_miri.py` calls this script with
`MIRI_BE_RUSTC=target` to compile a library and without it to interpret the
kernel binary. This script plays the role cargo-miri plays for cargo:

* proc-macro crates run inside the host compiler, so they are built by the
  host `rustc` (`KMIRI_HOST_RUSTC`, default `rustc`);
* every other crate goes to Miri (`KMIRI_MIRI`, default `miri`) for the
  `riscv64gc-unknown-linux-gnu` target;
* when interpreting, the KMiri machine description (`KMIRI_TOML`) and any
  extra interpreter flags (`KMIRI_FLAGS`, space separated) are appended.

Like `run_rustc.py`, the build script environment named by
`BUILD_SCRIPT_RUSTC_ENV_FILE` is merged into the compiler environment.
"""

import os
from pathlib import Path
import shlex
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                ".."))
from run_rustc import _ENV_FILE_VAR, _parse_env_file  # noqa: E402

# Same options as the BlueOS compiler's built-in riscv64 target.
TARGET = str(Path(__file__).resolve().with_name("riscv64-vivo-blueos.json"))


def crate_type(args):
    for i, arg in enumerate(args):
        if arg == "--crate-type" and i + 1 < len(args):
            return args[i + 1]
        if arg.startswith("--crate-type="):
            return arg.split("=", 1)[1]
    return None


def has_target(args):
    return any(a == "--target" or a.startswith("--target=") for a in args)


def main():
    args = sys.argv[1:]
    env = os.environ.copy()
    env_file = env.pop(_ENV_FILE_VAR, None)
    if env_file:
        env.update(_parse_env_file(env_file))

    if crate_type(args) == "proc-macro":
        compiler = env.get("KMIRI_HOST_RUSTC", "rustc")
        env.pop("MIRI_BE_RUSTC", None)
        os.execvpe(compiler, [compiler, *args], env)

    command = [env.get("KMIRI_MIRI", "miri")]
    if not has_target(args):
        command += ["--target", TARGET]
    # - The target is a JSON file, which needs -Zunstable-options.
    # - The sources target the BlueOS compiler, a slightly older nightly:
    #   features it still needs are stable here and some APIs are deprecated;
    #   `-D warnings` must not turn either into an error.
    # Compiler flags go before any `--`.
    at = args.index("--") if "--" in args else len(args)
    args = args[:at] + [
        "-Zunstable-options",
        "-Astable_features",
        "-Adeprecated",
    ] + args[at:]
    if "MIRI_BE_RUSTC" in env:
        command += args
    else:
        # Interpreting: program arguments follow `--`; interpreter flags must
        # come before it.
        if "--" in args:
            split = args.index("--")
            head, tail = args[:split], args[split:]
        else:
            head, tail = args, []
        extra = []
        toml = env.get("KMIRI_TOML")
        if toml:
            extra.append(f"-Zkmiri-toml={toml}")
        extra += shlex.split(env.get("KMIRI_FLAGS", ""))
        command += head + extra + tail
    os.execvpe(command[0], command, env)


if __name__ == "__main__":
    main()
