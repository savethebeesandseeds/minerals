$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$Manifest = Join-Path $PSScriptRoot "Cargo.toml"
$BuiltModule = Join-Path $PSScriptRoot "target\wasm32-unknown-unknown\release\minerals_map.wasm"
$MapAsset = Join-Path $PSScriptRoot "assets\world_forest_v1.bin"
$PublicMapDirectory = Join-Path $ProjectRoot "public-app\map"
$PublicModule = Join-Path $PublicMapDirectory "minerals_map.wasm"
$ExpectedAssetHash = "970e006dac8927e4aa7e659eab20d295f244aab92567f58caf629ce013a7a944"
$ExpectedModuleHash = "f095257a885fe1545c7ccf1b18e480da4685b0726f5b9a2532c2cecd6212799f"

if (-not (Test-Path -LiteralPath $MapAsset -PathType Leaf)) {
    throw "Expected map asset is missing: $MapAsset"
}
$AssetHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $MapAsset).Hash.ToLowerInvariant()
if ($AssetHash -ne $ExpectedAssetHash) {
    throw "Map asset hash is not the reviewed value: $AssetHash"
}

rustup run 1.96.0 cargo test --manifest-path $Manifest
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

rustup run 1.96.0 cargo build --manifest-path $Manifest --target wasm32-unknown-unknown --release
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

if (-not (Test-Path -LiteralPath $BuiltModule -PathType Leaf)) {
    throw "Expected WebAssembly output was not created: $BuiltModule"
}
$BuiltHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $BuiltModule).Hash.ToLowerInvariant()
if ($BuiltHash -ne $ExpectedModuleHash) {
    throw "WebAssembly build hash is not the reviewed value: $BuiltHash"
}

New-Item -ItemType Directory -Force $PublicMapDirectory | Out-Null
Copy-Item -LiteralPath $BuiltModule -Destination $PublicModule -Force

Write-Host "Built $PublicModule"
Write-Host "SHA-256 $BuiltHash"
