$projectRoot = Split-Path $PSScriptRoot -Parent
$localTools = Join-Path $projectRoot '.tools'
if (Test-Path "$localTools\cargo\bin\cargo.exe") {
    $env:CARGO_HOME = "$localTools\cargo"
    $env:RUSTUP_HOME = "$localTools\rustup"
    $env:PATH = "$localTools\cargo\bin;$env:PATH"
}
if (Test-Path "$localTools\msvc\VC\Tools\MSVC") {
    $vc = Get-ChildItem "$localTools\msvc\VC\Tools\MSVC" -Directory | Sort-Object Name -Descending | Select-Object -First 1
    $sdk = Get-ChildItem "$localTools\msvc\Windows Kits\10\Include" -Directory | Sort-Object Name -Descending | Select-Object -First 1
    $sdkRoot = "$localTools\msvc\Windows Kits\10"
    $env:VCToolsInstallDir = "$($vc.FullName)\"
    $env:VCINSTALLDIR = "$localTools\msvc\VC\"
    $env:VCToolsVersion = $vc.Name
    $env:WindowsSdkDir = "$sdkRoot\"
    $env:WindowsSDKVersion = "$($sdk.Name)\"
    $env:VSCMD_ARG_HOST_ARCH = 'x64'
    $env:VSCMD_ARG_TGT_ARCH = 'x64'
    $env:PATH = "$($vc.FullName)\bin\Hostx64\x64;$sdkRoot\bin\$($sdk.Name)\x64;$env:PATH"
    $env:INCLUDE = "$($vc.FullName)\include;$($sdk.FullName)\ucrt;$($sdk.FullName)\shared;$($sdk.FullName)\um;$($sdk.FullName)\winrt"
    $env:LIB = "$($vc.FullName)\lib\x64;$sdkRoot\Lib\$($sdk.Name)\ucrt\x64;$sdkRoot\Lib\$($sdk.Name)\um\x64"
    $env:CC = "$($vc.FullName)\bin\Hostx64\x64\cl.exe"
    $env:CXX = $env:CC
}
if (Test-Path "$localTools\pythonlib\clang\native\libclang.dll") {
    $env:LIBCLANG_PATH = "$localTools\pythonlib\clang\native"
}
if (Test-Path "$localTools\asio-sdk\ASIOSDK\common\asio.h") {
    $env:CPAL_ASIO_DIR = "$localTools\asio-sdk\ASIOSDK"
}
