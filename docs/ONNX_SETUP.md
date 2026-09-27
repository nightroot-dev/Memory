# ONNX / MiniLM setup (Windows)

Semantic embeddings via MiniLM require three things beyond the default
build: a working MSVC toolchain, the ONNX Runtime DLL, and the trained
projection matrix. This page covers all three. Total setup time is
about 10 minutes on a fresh machine.

## 1. MSVC toolchain

The default build uses the GNU toolchain (see `.cargo/config.toml`,
which pins `target = "x86_64-pc-windows-gnu"`). The ONNX path needs
MSVC because `ort-sys` ships prebuilt binaries for
`x86_64-pc-windows-msvc` only — there are no GNU prebuilts.

Both toolchains can coexist. Check:

    rustup toolchain list

You need `stable-x86_64-pc-windows-msvc` in the list. If it is missing:

    rustup toolchain install stable-x86_64-pc-windows-msvc

`link.exe` / `cl.exe` do **not** need to be on `PATH`. The MSVC
toolchain locates them via the Windows registry, provided Visual
Studio Build Tools (C++ workload) or Visual Studio is installed with
the Windows 10/11 SDK.

Verify the linker chain exists:

    Test-Path "C:\Program Files\Microsoft Visual Studio"
    Test-Path "C:\Program Files (x86)\Windows Kits\10"

If both are missing, install *Visual Studio 2022 Build Tools* with the
"C++ build tools" workload (~4 GB, requires admin), then retry.

## 2. ONNX Runtime DLL

The engine loads `onnxruntime.dll` (>= 1.24) at runtime via
`ort-load-dynamic`. Run the setup script — it finds an existing copy,
validates the version from the PE header, persists the path, or
downloads the latest release if nothing valid exists:

    powershell -ExecutionPolicy Bypass -File tools/setup_ort.ps1

Verify:

    Write-Host "current shell: $env:ORT_DYLIB_PATH"
    [Environment]::GetEnvironmentVariable("ORT_DYLIB_PATH", "User")

Both must print a path like
`C:\onnxruntime\onnxruntime-win-x64-1.30.0\lib\onnxruntime.dll`.
The User-scoped variable applies to new shells automatically.

Do **not** rely on `C:\Windows\System32\onnxruntime.dll` — the OS-shipped
copy (1.17.x as of 2026) is too old and will fail to load.

## 3. Build with the ONNX feature

The project config pins the GNU target globally, so every ONNX command
must override both toolchain and target on the command line:

    cargo +stable-x86_64-pc-windows-msvc --target x86_64-pc-windows-msvc check -p cmhm_core --features onnx

Note the flag order: `+toolchain` first, then the subcommand (`check`,
`test`, `build`), then `--target` and `--features`. Cargo rejects
`--target` placed before the subcommand.

Run the MiniLM tests:

    $env:ORT_DYLIB_PATH = [Environment]::GetEnvironmentVariable("ORT_DYLIB_PATH", "User")
    cargo +stable-x86_64-pc-windows-msvc --target x86_64-pc-windows-msvc test -p cmhm_core --features onnx --test minilm_e2e -- --nocapture

## 4. Projection matrix

`data/projections/minilm-l6-v2-int8.bin` (197,124 bytes) is the trained
384 → 128 PCA projection. It is git-ignored build output, not committed.
Regenerate it with:

    pip install fastembed numpy
    python tools/train_minilm_projection.py

`engine::start()` auto-loads every `*.bin` in `cfg.projections_dir`
(default `data/projections`), keyed by filename stem as `model_id`.
Learn and retrieve with `model_id = "minilm-l6-v2-int8"`.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `no prebuilt binaries available for target x86_64-pc-windows-gnu` | GNU target + `--features onnx` | Use the MSVC override command above |
| `could not find native static library onnxruntime` | Old `ort-download-binaries` feature | Already fixed — `Cargo.toml` uses `ort-load-dynamic` |
| `Failed to load onnxruntime` / `LoadLibraryExW` at runtime | `ORT_DYLIB_PATH` unset, wrong path, or 32-bit DLL | Re-run `tools/setup_ort.ps1`; confirm x64 path |
| `STATUS_DLL_NOT_FOUND` from `rustc` itself | Corrupt toolchain install | `rustup toolchain uninstall <t> && rustup toolchain install <t>` |
| `error: unexpected argument '--target'` | Flag placed before subcommand | Put `--target` after `check`/`test`/`build` |
