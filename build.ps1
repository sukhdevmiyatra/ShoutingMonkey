# Builds the release executables into dist\ (build.bat runs this).
#   dist\ShoutingMonkey.exe         - the desktop app (all most people need)
#   dist\shoutingmonkey-cli.exe     - the command-line version, for scripts and CI
#   dist\THIRD_PARTY_LICENSES.txt   - licences of the libraries compiled in
# Needs Rust (rustup). With the GNU toolchain it also needs a MinGW C
# compiler for the bundled SQLite; the MSVC toolchain uses Visual Studio's.
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
Set-Location $root

$gnu = $null -ne (rustc -vV | Select-String '^host:.*gnu')
if ($gnu -and -not (Get-Command gcc -ErrorAction SilentlyContinue)) {
    if (Test-Path 'C:\msys64\mingw64\bin\gcc.exe') {
        $env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
    } else {
        Write-Error 'A C compiler is needed. Install MSYS2 and run: pacman -S mingw-w64-x86_64-gcc'
    }
}

# Keep this machine's paths (user name, folder names) out of the binaries:
# Rust embeds source paths for error messages, so map them to neutral names.
# Flags are separated by the 0x1F character because the paths may contain spaces.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$flags = @(
    "--remap-path-prefix=$root=shoutingmonkey",
    "--remap-path-prefix=$cargoHome=cargo",
    "--remap-path-prefix=$rustupHome=rustup"
)
$env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1F

cargo build --release -p gui -p cli
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\ShoutingMonkey.exe, target\release\shoutingmonkey-cli.exe dist\ -Force

$python = Get-Command python -ErrorAction SilentlyContinue
if ($python) {
    & $python.Source tools\third_party_licenses.py --out dist\THIRD_PARTY_LICENSES.txt
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
    Write-Warning 'Python not found, so dist\THIRD_PARTY_LICENSES.txt was not written. Ship it with every release.'
}
Write-Host ''
Write-Host 'Built dist\ShoutingMonkey.exe, dist\shoutingmonkey-cli.exe and dist\THIRD_PARTY_LICENSES.txt'
