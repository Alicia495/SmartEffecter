# ビルド

## 環境

Windows x64、Rust MSVC（1.92以上、v1.0の実ビルドは1.99.1）、Visual Studio C++ Build Tools、Windows SDK、LLVM/libclang、ASIO SDKが必要です。ASIO SDKはGPLv3の選択肢を使用します。

```powershell
$env:CPAL_ASIO_DIR = 'C:\path\to\ASIOSDK'
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
. .\scripts\dev-env.ps1
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
.\scripts\build.ps1
```

`dist/GuitarSmartEffecter.exe` と依存ライセンスが生成されます。配布ビルドは、このPCのWindows App Controlとの互換性のため `cargo rustc --locked --bin guitar-smart-effecter -- -C opt-level=3` を使用します。アプリとDSPは最適化し、依存クレートはdevプロファイルです。一般的な環境では `cargo build --release --locked` も利用できます。

## v1.0の対応ソース

リリースの `SmartEffecter-v1.0-source.tar.gz` には、アプリのソース、Cargo.lockに対応する依存クレート、ASIO SDKのソース、ビルドスクリプトを含めます。Rust、MSVC、Windows SDK、LLVMなどの開発ツールは別途用意してください。

展開したソース内で次を設定すると、依存クレートとASIO SDKを追加ダウンロードせずにビルドできます。

```powershell
$env:CPAL_ASIO_DIR = (Resolve-Path '.\third_party\ASIOSDK').Path
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
cargo build --offline --locked
```

`.cargo/config.toml` が同梱の `vendor/` を参照します。異なるツールチェーンで生成されるバイナリのビット一致は保証しません。

## 診断

GUIアプリなので、PowerShellでは起動終了を待ち、結果をファイルへ出します。

```powershell
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--version --report version.json' -Wait
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--devices --report devices.json' -Wait
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--inputs --report inputs.json' -Wait
Start-Process .\dist\GuitarSmartEffecter.exe -ArgumentList '--outputs --report outputs.json' -Wait
```

`--probe 10` は実機を開きますが出力はミュートします。`--demo` は機器を使わない画面確認です。テスト設定を分離するには `GUITAR_EFFECTER_DATA_DIR` を指定します。
